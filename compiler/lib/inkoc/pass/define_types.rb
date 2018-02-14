# frozen_string_literal: true

module Inkoc
  module Pass
    # rubocop: disable Metrics/ClassLength
    class DefineTypes
      include VisitorMethods

      DeferredMethod = Struct.new(:ast, :scope)

      attr_reader :module

      def initialize(mod, state)
        @module = mod
        @state = state
        @method_bodies = []
      end

      def diagnostics
        @state.diagnostics
      end

      def typedb
        @state.typedb
      end

      def define_type(node, scope)
        type = process_node(node, scope)
        node.type = type if type
      end

      def define_types(nodes, scope)
        nodes.map { |node| define_type(node, scope) }
      end

      def run(ast)
        locals = ast.locals

        on_module_body(ast, locals)

        # Method bodies are processed last since they may depend on types
        # defined after the method itself is defined.
        @method_bodies.each do |method|
          process_deferred_method(method)
        end

        [ast]
      end

      def process_imports(scope)
        @module.imports.each do |node|
          process_node(node, scope)
        end
      end

      def on_module_body(ast, locals)
        @module.globals.define(Config::MODULE_GLOBAL, @module.type)

        scope = TypeScope.new(@module.type, @module.body.type, locals)

        process_imports(scope)

        define_type(ast, scope)
      end

      def on_import(node, _)
        name = node.qualified_name
        mod = @state.module(name)

        node.symbols.each do |import_symbol|
          process_node(import_symbol, mod)
        end
      end

      def on_import_symbol(symbol, source_mod)
        return unless symbol.expose?

        sym_name = symbol.symbol_name.name
        type = source_mod.type_of_attribute(sym_name)
        import_as = symbol.import_as(source_mod)

        unless type
          diagnostics.import_undefined_symbol_error(
            source_mod.name,
            sym_name,
            symbol.location
          )

          return
        end

        import_symbol_as_global(import_as, type, symbol.location_for_name)
      end

      def on_import_self(symbol, source_mod)
        return unless symbol.expose?

        import_as = symbol.import_as(source_mod)
        loc = symbol.location_for_name

        import_symbol_as_global(import_as, source_mod.type, loc)
      end

      def on_import_glob(symbol, source_mod)
        loc = symbol.location_for_name

        source_mod.attributes.each do |attribute|
          import_symbol_as_global(attribute.name, attribute.type, loc)
        end
      end

      def import_symbol_as_global(name, type, location)
        if @module.global_defined?(name)
          diagnostics.import_existing_symbol_error(name, location)
        else
          @module.globals.define(name, type)
        end
      end

      def on_body(node, scope)
        scope.define_self_local

        return_types = return_types_for_body(node, scope)
        first_type = return_types[0][0]

        return_types.each do |(type, location)|
          next if type.type_compatible?(first_type)

          diagnostics.type_error(first_type, type, location)
        end

        first_type
      end

      def return_types_for_body(node, scope)
        types = []
        last_type = nil

        node.expressions.each do |expr|
          type = define_type(expr, scope)

          next unless type

          location = expr.location
          last_type = [type, location]

          types.push([type, location]) if expr.return?
        end

        last_type ||= [typedb.nil_type, node.location]

        types << last_type
      end

      def on_integer(*)
        typedb.integer_type
      end

      def on_float(*)
        typedb.float_type
      end

      def on_string(*)
        typedb.string_type
      end

      def on_attribute(node, scope)
        name = node.name
        symbol = scope.self_type.lookup_attribute(name)

        if symbol.nil?
          diagnostics
            .undefined_attribute_error(scope.self_type, name, node.location)
        end

        symbol.type
      end

      def on_constant(node, scope)
        resolve_module_type(node, scope.self_type)
      end

      def on_identifier(node, scope)
        name = node.name
        loc = node.location
        self_type = scope.self_type

        rtype, block_type =
          if (depth_sym = scope.depth_and_symbol_for_local(name))
            node.depth = depth_sym[0]
            node.symbol = depth_sym[1]
            node.symbol.type
          elsif self_type.responds_to_message?(name)
            send_object_message(self_type, name, [], scope, loc)
          elsif @module.responds_to_message?(name)
            send_object_message(@module.type, name, [], scope, loc)
          elsif (global_type = @module.type_of_global(name))
            global_type
          else
            diagnostics.undefined_method_error(self_type, name, loc)
            Type::Dynamic.new
          end

        node.block_type = block_type if block_type

        rtype.resolve_type(self_type)
      end

      def on_global(node, *)
        name = node.name
        symbol = @module.globals[name]

        diagnostics.undefined_constant_error(name, node.location) if symbol.nil?

        symbol.type
      end

      def on_self(_, scope)
        scope.self_type
      end

      def on_send(node, scope)
        rtype, node.block_type = send_object_message(
          receiver_type(node, scope),
          node.name,
          node.arguments,
          scope,
          node.location
        )

        rtype
      end

      def on_keyword_argument(node, scope)
        define_type(node.value, scope)
      end

      def send_object_message(receiver, name, args, scope, location)
        if receiver.dynamic?
          define_types(args, scope)

          return receiver
        end

        if receiver.unresolved_constraint?
          arg_types = define_types(args, scope)

          return receiver
              .define_required_method(receiver, name, arg_types, typedb)
              .returns
        end

        unless receiver.responds_to_message?(name)
          define_types(args, scope)

          rtype =
            if handle_unknown_message?(receiver)
              receiver.unknown_message_return_type
            else
              diagnostics.undefined_method_error(receiver, name, location)

              Type::Dynamic.new
            end

          return rtype
        end

        symbol = receiver.lookup_method(name)
        method_type = symbol.type

        context = MessageContext
          .new(receiver, method_type, args, scope, location)

        verify_send_arguments(context)

        [context.initialized_return_type, method_type]
      end

      def handle_unknown_message?(receiver)
        method = receiver.lookup_method(Config::UNKNOWN_MESSAGE_MESSAGE)

        return false if method.nil?

        arg_types = method.type.argument_types_without_self
        rest_type = typedb.new_array_of_type(Type::Dynamic.new)

        arg_types.length == 2 &&
          arg_types[0].type_compatible?(typedb.string_type) &&
          arg_types[1].type_compatible?(rest_type)
      end

      def verify_send_arguments(context)
        return unless verify_keyword_arguments(context)

        given_count = context.arguments.length

        if context.valid_number_of_arguments?(given_count)
          verify_send_argument_types(context)
        else
          diagnostics.argument_count_error(
            given_count,
            context.argument_count_range,
            context.location
          )
        end
      end

      def verify_keyword_arguments(context)
        context.arguments.all? do |arg|
          next true unless arg.keyword_argument?

          name = arg.name

          next true if context.valid_argument_name?(name)

          diagnostics
            .undefined_keyword_argument_error(name, context.block, arg.location)

          false
        end
      end

      def verify_send_argument_types(context)
        max_args = context.arguments_count_without_self
        has_rest = context.rest_argument

        context.arguments.each_with_index do |arg, index|
          # We add 1 to the index to skip the self argument.
          arg_index = index + 1
          aname = arg.keyword_argument? ? arg.name : arg_index
          rest = arg_index >= max_args && has_rest

          define_given_type(context, arg, aname, rest)

          expected =
            expected_type_for_argument(context, aname, arg.type, rest)

          if rest
            verify_rest_argument(context, arg, expected, arg.location)
          else
            verify_send_argument(arg, expected, arg.location)
          end
        end
      end

      def define_given_type(context, arg, name, rest = false)
        expected = context.type_for_argument_or_rest(name, rest)

        # We don't define the argument type until here so we can correctly infer
        # closures without signatures (e.g. `{ 10 }`) as lambdas.
        arg.infer_as_lambda if expected.lambda? && arg.block_without_signature?

        define_type(arg, context.type_scope)
      end

      # context - The MessageContext of the current call being validated.
      # aname - The name (or index) of the argument we're validating.
      # type - The type of the argument that is being validated.
      # rest - If true the argument is supposed to be passed to a rest argument.
      def expected_type_for_argument(context, aname, type, rest = false)
        context.type_for_argument_or_rest(aname, rest)
          .resolve_type(context.receiver)
          .initialize_as(type, context)
      end

      def verify_send_argument(argument, expected, location)
        given = argument.type

        if expected.type_parameter? && !given.implements_trait?(expected)
          diagnostics
            .type_parameter_not_implemented_error(expected, given, location)

          return
        end

        given.infer_to(expected) if infer_block?(given, expected)

        return if given.type_compatible?(expected)

        diagnostics.type_error(expected, given, location)
      end

      def verify_rest_argument(context, argument, rest_type, location)
        arg_type = argument.type
        expected = rest_type
          .lookup_type_parameter_instance(Config::ARRAY_TYPE_PARAMETER)
          .initialize_as(arg_type, context)

        return if arg_type.type_compatible?(expected)

        diagnostics.type_error(expected, arg_type, location)
      end

      def infer_block?(given, expected)
        given.block? && expected.block? && given.infer?
      end

      def receiver_type(node, scope)
        name = node.name

        node.receiver_type =
          if node.receiver
            define_type(node.receiver, scope)
          elsif scope.self_type.lookup_method(name).any?
            scope.self_type
          elsif @module.globals[name].any?
            @module.type
          else
            scope.self_type
          end
      end

      def on_raw_instruction(node, scope)
        callback = node.raw_instruction_visitor_method

        # Although we don't directly use the argument types here we still want
        # to store them in every node so we can access them later on.
        node.arguments.each { |arg| define_type(arg, scope) }

        if respond_to?(callback)
          public_send(callback, node, scope)
        else
          diagnostics.unknown_raw_instruction_error(node.name, node.location)
          typedb.nil_type
        end
      end

      def on_raw_get_toplevel(*)
        typedb.top_level
      end

      def on_raw_set_prototype(node, *)
        node.arguments.fetch(1).type
      end

      def on_raw_set_attribute(node, *)
        node.arguments.fetch(2).type
      end

      def on_raw_set_attribute_to_object(*)
        typedb.new_empty_object
      end

      def on_raw_get_attribute(node, *)
        object = node.arguments.fetch(0).type
        name = node.arguments.fetch(1)

        if name.string?
          object.lookup_attribute(name.value).type
        else
          Type::Dynamic.new
        end
      end

      def on_raw_set_object(node, *)
        proto =
          if (proto_node = node.arguments[1])
            proto_node.type
          else
            typedb.object_type
          end

        proto = proto.type if proto.optional?

        Type::Object.new(prototype: proto)
      end

      def on_raw_object_equals(*)
        typedb.boolean_type
      end

      def on_raw_object_is_kind_of(*)
        typedb.boolean_type
      end

      def on_raw_copy_blocks(*)
        Type::Void.new
      end

      def on_raw_prototype_chain_attribute_contains(*)
        typedb.boolean_type
      end

      def on_raw_integer_to_string(*)
        typedb.string_type
      end

      def on_raw_integer_to_float(*)
        typedb.float_type
      end

      def on_raw_integer_add(*)
        typedb.integer_type
      end

      def on_raw_integer_div(*)
        typedb.integer_type
      end

      def on_raw_integer_mul(*)
        typedb.integer_type
      end

      def on_raw_integer_sub(*)
        typedb.integer_type
      end

      def on_raw_integer_mod(*)
        typedb.integer_type
      end

      def on_raw_integer_bitwise_and(*)
        typedb.integer_type
      end

      def on_raw_integer_bitwise_or(*)
        typedb.integer_type
      end

      def on_raw_integer_bitwise_xor(*)
        typedb.integer_type
      end

      def on_raw_integer_shift_left(*)
        typedb.integer_type
      end

      def on_raw_integer_shift_right(*)
        typedb.integer_type
      end

      def on_raw_integer_smaller(*)
        typedb.boolean_type
      end

      def on_raw_integer_greater(*)
        typedb.boolean_type
      end

      def on_raw_integer_equals(*)
        typedb.boolean_type
      end

      def on_raw_integer_greater_or_equal(*)
        typedb.boolean_type
      end

      def on_raw_integer_smaller_or_equal(*)
        typedb.boolean_type
      end

      def on_raw_float_to_string(*)
        typedb.string_type
      end

      def on_raw_float_to_integer(*)
        typedb.integer_type
      end

      def on_raw_float_add(*)
        typedb.float_type
      end

      def on_raw_float_div(*)
        typedb.float_type
      end

      def on_raw_float_mul(*)
        typedb.float_type
      end

      def on_raw_float_sub(*)
        typedb.float_type
      end

      def on_raw_float_mod(*)
        typedb.float_type
      end

      def on_raw_float_smaller(*)
        typedb.boolean_type
      end

      def on_raw_float_greater(*)
        typedb.boolean_type
      end

      def on_raw_float_equals(*)
        typedb.boolean_type
      end

      def on_raw_float_greater_or_equal(*)
        typedb.boolean_type
      end

      def on_raw_float_smaller_or_equal(*)
        typedb.boolean_type
      end

      def on_raw_float_is_nan(*)
        typedb.boolean_type
      end

      def on_raw_float_is_infinite(*)
        typedb.boolean_type
      end

      def on_raw_float_ceil(*)
        typedb.float_type
      end

      def on_raw_float_floor(*)
        typedb.float_type
      end

      def on_raw_float_round(*)
        typedb.float_type
      end

      def on_raw_stdout_write(*)
        typedb.integer_type
      end

      def on_raw_get_boolean_prototype(*)
        typedb.boolean_type
      end

      def on_raw_get_true(*)
        typedb.true_type
      end

      def on_raw_get_false(*)
        typedb.false_type
      end

      def on_raw_get_nil(*)
        typedb.nil_type
      end

      def on_raw_run_block(node, *)
        node.arguments[0].type.return_type
      end

      def on_raw_get_string_prototype(*)
        typedb.string_type
      end

      def on_raw_get_integer_prototype(*)
        typedb.integer_type
      end

      def on_raw_get_float_prototype(*)
        typedb.float_type
      end

      def on_raw_get_object_prototype(*)
        typedb.object_type
      end

      def on_raw_get_array_prototype(*)
        typedb.array_type
      end

      def on_raw_get_block_prototype(*)
        typedb.block_type
      end

      def optional_array_element_value(array)
        param = Config::ARRAY_TYPE_PARAMETER
        type =
          array.lookup_type_parameter_instance_or_parameter(param) ||
          Type::Dynamic.new

        Type::Optional.new(type)
      end

      def on_raw_array_length(*)
        typedb.integer_type
      end

      def on_raw_array_at(node, _)
        array = node.arguments.fetch(0).type

        optional_array_element_value(array)
      end

      def on_raw_array_set(node, _)
        node.arguments.fetch(2).type
      end

      def on_raw_array_clear(*)
        Type::Void.new
      end

      def on_raw_array_remove(node, _)
        array = node.arguments.fetch(0).type

        optional_array_element_value(array)
      end

      def on_raw_time_monotonic(*)
        typedb.float_type
      end

      def on_raw_time_system(*)
        typedb.date_time_type
      end

      def on_raw_time_get_value(*)
        typedb.integer_type
      end

      def on_raw_string_to_upper(*)
        typedb.string_type
      end

      def on_raw_string_to_lower(*)
        typedb.string_type
      end

      def on_raw_string_to_bytes(*)
        typedb.new_array_of_type(typedb.integer_type)
      end

      def on_raw_string_size(*)
        typedb.integer_type
      end

      def on_raw_string_length(*)
        typedb.integer_type
      end

      def on_raw_string_equals(*)
        typedb.boolean_type
      end

      def on_raw_string_from_bytes(*)
        typedb.string_type
      end

      def on_raw_stdin_read_line(*)
        typedb.string_type
      end

      def on_raw_stdin_read(*)
        typedb.string_type
      end

      def on_raw_stdin_read_exact(*)
        typedb.string_type
      end

      def on_raw_stderr_write(*)
        typedb.integer_type
      end

      def on_raw_process_spawn(*)
        typedb.integer_type
      end

      def on_raw_process_send_message(node, _)
        node.arguments.fetch(1).type
      end

      def on_raw_process_receive_message(*)
        Type::Dynamic.new
      end

      def on_raw_process_current_pid(*)
        typedb.integer_type
      end

      def on_raw_process_status(*)
        typedb.integer_type
      end

      def on_raw_process_suspend_current(*)
        Type::Void.new
      end

      def on_raw_remove_attribute(node, _)
        object = node.arguments.fetch(0).type
        name = node.arguments.fetch(1)

        if name.string?
          object.lookup_attribute(name.value).type
        else
          Type::Dynamic.new
        end
      end

      def on_raw_get_prototype(node, _)
        proto = node.arguments.fetch(0).type.prototype || typedb.nil_type

        Type::Optional.new(proto)
      end

      def on_raw_get_attribute_names(*)
        typedb.new_array_of_type(typedb.string_type)
      end

      def on_raw_attribute_exists(*)
        typedb.boolean_type
      end

      def on_raw_file_flush(*)
        typedb.nil_type
      end

      def on_raw_stdout_flush(*)
        typedb.nil_type
      end

      def on_raw_stderr_flush(*)
        typedb.nil_type
      end

      def on_raw_file_open(*)
        typedb.file_type
      end

      def on_raw_file_read(*)
        typedb.string_type
      end

      def on_raw_file_read_line(*)
        typedb.string_type
      end

      def on_raw_file_read_exact(*)
        typedb.string_type
      end

      def on_raw_file_seek(*)
        typedb.integer_type
      end

      def on_raw_file_size(*)
        typedb.integer_type
      end

      def on_raw_file_write(*)
        typedb.integer_type
      end

      def on_raw_file_remove(*)
        typedb.nil_type
      end

      def on_raw_file_copy(*)
        typedb.integer_type
      end

      def on_raw_file_type(*)
        typedb.integer_type
      end

      def on_raw_file_time(*)
        typedb.date_time_type
      end

      def on_raw_drop(*)
        typedb.nil_type
      end

      def on_raw_move_to_pool(*)
        typedb.integer_type
      end

      def on_raw_panic(*)
        typedb.void_type
      end

      def on_raw_exit(*)
        typedb.void_type
      end

      def on_raw_platform(*)
        typedb.string_type
      end

      def on_return(node, scope)
        if node.value
          define_type(node.value, scope)
        else
          typedb.nil_type
        end
      end

      def on_throw(node, scope)
        throw_type = define_type(node.value, scope)

        # For block types we infer the throw type so one doesn't have to
        # annotate every block with an explicit type.
        scope.block_type.throws ||= throw_type if scope.closure?

        typedb.void_type
      end

      def on_try(node, scope)
        node.try_block_type =
          block_type_with_self(Config::TRY_BLOCK_NAME, scope.self_type)

        node.else_block_type =
          block_type_with_self(Config::ELSE_BLOCK_NAME, scope.self_type)

        try_scope =
          TypeScope.new(scope.self_type, node.try_block_type, scope.locals)

        try_type =
          node.try_block_type.return_type_for_block_and_call =
            define_type(node.expression, try_scope)

        else_scope = node.type_scope_for_else(scope.self_type)

        node.define_else_argument_type

        else_type = else_type_for_try(node, else_scope)

        if try_type.physical_type? &&
           else_type.physical_type? &&
           !else_type.nil_type? &&
           !else_type.type_compatible?(try_type)
          diagnostics.type_error(try_type, else_type, node.else_body.location)
        end

        rtype = try_type.if_physical_or_else { else_type }

        if else_type.nil_type?
          Type::Optional.new(rtype)
        else
          rtype
        end
      end

      def else_type_for_try(node, scope)
        if node.explicit_block_for_else_body? && node.else_body.empty?
          node.else_body.type = typedb.nil_type
        elsif node.else_body.empty?
          node.else_body.type = Type::Void.new
        else
          define_type(node.else_body, scope)
        end
      end

      def block_type_with_self(name, self_type)
        type = Type::Block.new(name: name, prototype: typedb.block_type)

        type.define_self_argument(self_type)
        type
      end

      def on_object(node, scope)
        name = node.name
        proto = typedb.object_type
        type = typedb.new_object_type(name, proto)

        type.define_attribute(
          Config::OBJECT_NAME_INSTANCE_ATTRIBUTE,
          typedb.string_type
        )

        block_type = define_block_type_for_object(node, type)
        new_scope = TypeScope.new(type, block_type, node.body.locals)

        define_type_parameters(node.type_parameters, type)
        store_type(type, scope.self_type, node.location)
        define_type(node.body, new_scope)

        type
      end

      def define_block_type_for_object(node, type)
        node.block_type = Type::Block.new(
          prototype: typedb.block_type,
          returns: node.body.type
        )

        node.block_type.define_self_argument(type)
        node.block_type
      end

      def on_trait(node, scope)
        name = node.name
        type = Type::Trait.new(name: name, prototype: typedb.trait_type)

        define_type_parameters(node.type_parameters, type)

        node.required_traits.each do |trait|
          trait_type = resolve_module_type(trait, scope.self_type)
          type.required_traits << trait_type if trait_type.trait?
        end

        block_type = define_block_type_for_object(node, type)
        new_scope = TypeScope.new(type, block_type, node.body.locals)

        store_type(type, scope.self_type, node.location)
        define_type(node.body, new_scope)

        type
      end

      def on_trait_implementation(node, scope)
        self_type = scope.self_type
        loc = node.location

        trait = resolve_module_type(node.trait_name, self_type)

        node.object_names.each do |object_name|
          object = resolve_module_type(object_name, self_type)

          verify_same_type_parameters(object_name, object)

          param_instances =
            type_parameter_instances_for(node.trait_name, object)

          block_type = define_block_type_for_object(node, object)
          new_scope = TypeScope.new(object, block_type, node.body.locals)

          # We add the trait to the object first so type checks comparing the
          # object and trait will pass.
          object.implemented_traits << trait

          define_type(node.body, new_scope)

          traits_implemented = required_traits_implemented?(object, trait, loc)

          methods_implemented =
            required_methods_implemented?(object, trait, param_instances, loc)

          unless traits_implemented && methods_implemented
            object.implemented_traits.delete(trait)
          end
        end

        trait
      end

      def type_parameter_instances_for(node, self_type)
        node.type_parameters.map do |name|
          resolve_module_type(name, self_type)
        end
      end

      def on_reopen_object(node, scope)
        self_type = scope.self_type
        object = resolve_module_type(node.name, self_type)
        block_type = define_block_type_for_object(node, object)
        new_scope = TypeScope.new(object, block_type, node.body.locals)

        verify_same_type_parameters(node.name, object)

        define_type(node.body, new_scope)
      end

      def verify_same_type_parameters(node, type)
        node_names = node.type_parameters.map(&:name)
        type_names = type.type_parameter_names

        return if node_names == type_names

        diagnostics
          .invalid_type_parameters(type, node_names, node.location)
      end

      def required_traits_implemented?(object, trait, location)
        trait.required_traits.each do |req_trait|
          next if object.implements_trait?(req_trait)

          diagnostics
            .uninplemented_trait_error(trait, object, req_trait, location)

          return false
        end

        true
      end

      def required_methods_implemented?(object, trait, param_instances, loc)
        trait.required_method_types(param_instances).each do |method_type|
          next if object.implements_method?(method_type)

          diagnostics.unimplemented_method_error(method_type, object, loc)

          return false
        end

        true
      end

      def on_method(node, scope)
        self_type = scope.self_type

        type = Type::Block.new(
          name: node.name,
          prototype: typedb.block_type,
          block_type: :method
        )

        new_scope = TypeScope.new(self_type, type, node.body.locals)

        block_signature(node, type, new_scope)

        if node.required?
          if self_type.trait?
            self_type.define_required_method(type)
          else
            diagnostics.define_required_method_on_non_trait_error(node.location)
          end
        else
          store_type(type, self_type, node.location)

          @method_bodies << DeferredMethod.new(node, new_scope)
        end

        type
      end

      def process_deferred_method(method)
        node = method.ast
        body = node.body

        define_type(body, method.scope)

        expected_type = node.type
          .return_type
          .resolve_type(method.scope.self_type)

        inferred_type = body.type

        return if inferred_type.type_compatible?(expected_type)

        diagnostics
          .return_type_error(expected_type, inferred_type, node.location)
      end

      def on_block(node, scope)
        if node.lambda?
          block_name = Config::LAMBDA_TYPE_NAME
          block_type = :lambda
          self_type = @module.type
        else
          block_name = Config::BLOCK_TYPE_NAME
          block_type = :closure
          self_type = scope.self_type
        end

        type = Type::Block.new(
          name: block_name,
          prototype: typedb.block_type,
          block_type: block_type
        )

        new_scope = TypeScope.new(self_type, type, node.body.locals)

        block_signature(node, type, new_scope, constraints: true)
        define_type(node.body, new_scope)

        rtype = node.body.type
        exp = type.return_type.resolve_type(scope.self_type)

        type.return_type_for_block_and_call = rtype if type.returns.dynamic?

        unless rtype.type_compatible?(exp)
          diagnostics.return_type_error(exp, rtype, node.location)
        end

        type
      end
      alias on_lambda on_block

      def on_type_cast(node, scope)
        define_type(node.expression, scope)

        params = node.cast_to.type_parameters.map do |param|
          resolve_module_type(param, scope.self_type)
        end

        rtype = resolve_module_type(node.cast_to, scope.self_type)
          .new_instance(params)

        wrap_optional_type(node.cast_to, rtype)
      end

      def on_define_variable(node, scope)
        callback = node.variable.define_variable_visitor_method
        vtype = define_type(node.value, scope)

        if node.value_type
          exp_type = resolve_module_type(node.value_type, scope.self_type)

          # If an explicit type is given and the inferred type is compatible we
          # want to use the _explicit type_ as _the_ type, instead of the
          # inferred one.
          if vtype.type_compatible?(exp_type)
            vtype = exp_type
          else
            diagnostics.type_error(exp_type, vtype, node.location)
          end
        end

        public_send(callback, node, vtype, scope)

        node.variable.type = vtype
      end

      def on_define_constant(node, value_type, scope)
        name = node.variable.name
        store_type(value_type, scope.self_type, node.location, name)
      end

      def on_define_attribute(node, value_type, scope)
        var = node.variable

        if scope.method? && scope.block_type.name == Config::INIT_MESSAGE
          scope.self_type.define_attribute(node.variable.name, value_type)
        else
          diagnostics.define_instance_attribute_error(var.name, var.location)
        end
      end

      def on_define_local(node, value_type, scope)
        scope.locals.define(node.variable.name, value_type, node.mutable?)
      end

      def on_reassign_variable(node, scope)
        callback = node.variable.reassign_variable_visitor_method
        vtype = define_type(node.value, scope)

        public_send(callback, node, vtype, scope)

        node.variable.type = vtype
      end

      def on_reassign_attribute(node, value_type, scope)
        name = node.variable.name
        symbol = scope.self_type.lookup_attribute(name)
        existing_type = symbol.type

        if symbol.nil?
          diagnostics.reassign_undefined_attribute_error(name, node.location)
          return existing_type
        end

        unless symbol.mutable?
          diagnostics.reassign_immutable_attribute_error(name, node.location)
          return existing_type
        end

        return if value_type.type_compatible?(existing_type)

        diagnostics.type_error(existing_type, value_type, node.value.location)
      end

      def on_reassign_local(node, value_type, scope)
        name = node.variable.name
        _, local = scope.locals.lookup_with_parent(name)
        existing_type = local.type

        if local.nil?
          diagnostics.reassign_undefined_local_error(name, node.location)
          return existing_type
        end

        unless local.mutable?
          diagnostics.reassign_immutable_local_error(name, node.location)
          return existing_type
        end

        return if value_type.type_compatible?(existing_type)

        diagnostics.type_error(existing_type, value_type, node.value.location)
      end

      def block_signature(node, type, scope, constraints: false)
        define_type_parameters(node.type_parameters, type)
        define_arguments(node.arguments, type, scope, constraints: constraints)
        define_return_type(node, type, scope.self_type)
        define_throw_type(node, type, scope.self_type)
        verify_block_type_parameters(node.type_parameters, scope.self_type)
        type.define_call_method
      end

      def verify_block_type_parameters(params, receiver_type)
        existing = receiver_type.type_parameter_names.to_set

        params.each do |param|
          next unless existing.include?(param.name)

          diagnostics.shadowing_type_parameter_error(param.name, param.location)
        end
      end

      def define_arguments(arguments, block_type, scope, constraints: false)
        self_symbol = block_type
          .define_self_argument(scope.self_type)

        scope.locals.add_symbol(self_symbol)

        arguments.each do |arg|
          val_type = type_for_argument_value(arg, scope)
          def_type = defined_type_for_argument(arg, block_type, scope.self_type)

          # If both an explicit type and default value are given we need to make
          # sure the two are compatible.
          if argument_types_incompatible?(def_type, val_type)
            diagnostics.type_error(def_type, val_type, arg.default.location)
          end

          arg_name = arg.name
          mutable = arg.mutable?
          arg_type =
            def_type ||
            val_type ||
            default_argument_type(constraints: constraints)

          arg_type = arg_type

          arg_symbol =
            if arg.default
              block_type.define_argument(arg_name, arg_type, mutable)
            elsif arg.rest?
              rest_type = typedb.new_array_of_type(arg_type)
              block_type.define_rest_argument(arg_name, rest_type, mutable)
            else
              block_type.define_required_argument(arg_name, arg_type, mutable)
            end

          arg.type = arg_type

          scope.locals.add_symbol(arg_symbol)
        end
      end

      def default_argument_type(constraints: false)
        if constraints
          Type::Constraint.new
        else
          Type::Dynamic.new
        end
      end

      def define_return_type(node, block_type, self_type)
        rnode = node.returns

        unless rnode
          block_type.return_type_for_block_and_call = Type::Dynamic.new
          return
        end

        if rnode.self_type?
          block_type.return_type_for_block_and_call = Type::SelfType.new
          return
        end

        rtype = resolve_type(rnode, self_type, [block_type, self_type, @module])

        block_type.return_type_for_block_and_call =
          wrap_optional_type(rnode, rtype)
      end

      def define_throw_type(node, block_type, self_type)
        return unless node.throws

        ttype =
          resolve_type(node.throws, self_type, [block_type, self_type, @module])

        block_type.throws = wrap_optional_type(node.throws, ttype)
      end

      def type_for_argument_value(arg, scope)
        define_type(arg.default, scope) if arg.default
      end

      def defined_type_for_argument(arg, block_type, self_type)
        unless (vtype = arg.value_type)
          return
        end

        wrap_optional_type(
          vtype,
          resolve_type(vtype, self_type, [block_type, self_type, @module])
        )
      end

      def argument_types_incompatible?(defined_type, value_type)
        defined_type && value_type && !value_type.type_compatible?(defined_type)
      end

      def store_type(type, self_type, location, name = type.name)
        self_type.define_attribute(name, type)

        if Config::RESERVED_CONSTANTS.include?(name)
          diagnostics.redefine_reserved_constant_error(name, location)
        end

        @module.globals.define(name, type) if module_scope?(self_type)
      end

      def module_scope?(self_type)
        self_type == @module.type
      end

      def wrap_optional_type(node, type)
        node.optional? ? Type::Optional.new(type) : type
      end

      def define_type_parameters(arguments, type)
        arguments.each do |arg_node|
          required_traits = arg_node.required_traits.map do |node|
            resolve_type(node, type, [type, self.module])
          end

          type.define_type_parameter(arg_node.name, required_traits)
        end
      end

      def resolve_module_type(node, self_type)
        resolve_type(node, self_type, [self_type, @module])
      end

      def resolve_type(node, self_type, sources)
        return Type::SelfType.new if node.self_type?
        return Type::Dynamic.new if node.dynamic_type?
        return Type::Void.new if node.void_type?

        if node.lambda_or_block_type?
          return resolve_block_type(node, self_type, sources, node.lambda_type?)
        end

        name = node.name

        if node.receiver
          receiver = resolve_type(node.receiver, self_type, sources)
          sources = [receiver] + sources
        end

        sources.find do |source|
          if (type = source.lookup_type(name))
            return type
          end
        end

        diagnostics.undefined_constant_error(node.name, node.location)

        Type::Dynamic.new
      end

      def resolve_block_type(node, self_type, sources, is_lambda = false)
        args = node.arguments.map do |arg|
          resolve_type(arg, self_type, sources)
        end

        returns =
          if (rnode = node.returns)
            resolve_type(rnode, self_type, sources)
          end

        throws =
          if (tnode = node.throws)
            resolve_type(tnode, self_type, sources)
          end

        type = Type::Block.new(
          name: is_lambda ? Config::LAMBDA_TYPE_NAME : Config::BLOCK_TYPE_NAME,
          prototype: typedb.block_type,
          returns: returns,
          throws: throws,
          block_type: is_lambda ? :lambda : :closure
        )

        type.define_self_argument(self_type)

        args.each_with_index do |arg, index|
          type.define_argument(index.to_s, arg)
        end

        type.define_call_method

        wrap_optional_type(node, type)
      end

      def inspect
        # The default inspect is very slow, slowing down the rendering of any
        # runtime errors.
        '#<Pass::DefineTypes>'
      end
    end
    # rubocop: enable Metrics/ClassLength
  end
end
