# frozen_string_literal: true

module Inkoc
  module Pass
    class DefineTypes
      include VisitorMethods

      DeferredMethod = Struct.new(:ast, :self_type, :locals)

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

      def define_type(node, self_type, locals)
        node.type = process_node(node, self_type, locals)
      end

      def define_types(nodes, self_type, locals)
        nodes.map { |node| define_type(node, self_type, locals) }
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

      def on_module_body(ast, locals)
        @module.type =
          if @module.define_module?
            define_module_type
          else
            typedb.top_level
          end

        @module.globals.define(Config::MODULE_GLOBAL, @module.type)

        define_type(ast, @module.type, locals)
      end

      def define_module_type
        top = typedb.top_level
        modules = top.lookup_attribute(Config::MODULES_ATTRIBUTE).type
        proto = top.lookup_attribute(Config::MODULE_TYPE).type
        type = Type::Object.new(name: @module.name.to_s, prototype: proto)

        modules.define_attribute(type.name, type, true)

        type
      end

      def on_body(node, self_type, locals)
        locals.define(Config::SELF_LOCAL, self_type)

        return_types = return_types_for_body(node, self_type, locals)
        first_type = return_types[0][0]

        return_types.each do |(type, location)|
          next if type.type_compatible?(first_type)

          diagnostics.type_error(first_type, type, location)
        end

        first_type
      end

      def return_types_for_body(node, self_type, locals)
        types = []
        last_type = nil

        node.expressions.each do |expr|
          type = define_type(expr, self_type, locals)

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

      def on_attribute(node, self_type, *)
        name = node.name
        symbol = self_type.lookup_attribute(name)

        if symbol.nil?
          diagnostics.undefined_attribute_error(self_type, name, node.location)
        end

        symbol.type
      end

      def on_constant(node, self_type, *)
        name = node.name
        symbol = self_type.lookup_attribute(name)
          .or_else { @module.globals[name] }

        diagnostics.undefined_constant_error(name, node.location) if symbol.nil?

        symbol.type
      end

      def on_identifier(node, self_type, locals)
        name = node.name
        loc = node.location

        type =
          if (local = locals[name]) && local.any?
            local.type
          elsif self_type.responds_to_message?(name)
            send_object_message(self_type, name, [], self_type, locals, loc)
          elsif @module.responds_to_message?(name)
            send_object_message(@module.type, name, [], self_type, locals, loc)
          elsif (global_type = @module.type_of_global(name))
            global_type
          else
            diagnostics.undefined_method_error(self_type, name, loc)
            Type::Dynamic.new
          end

        type.resolve_type(self_type)
      end

      def on_global(node, *)
        name = node.name
        symbol = @module.globals[name]

        diagnostics.undefined_constant_error(name, node.location) if symbol.nil?

        symbol.type
      end

      def on_self(_, self_type, *)
        self_type
      end

      def on_send(node, self_type, locals)
        send_object_message(
          receiver_type(node, self_type, locals),
          node.name,
          node.arguments,
          self_type,
          locals,
          node.location
        )
      end

      def on_keyword_argument(node, self_type, locals)
        define_type(node.value, self_type, locals)
      end

      def send_object_message(receiver, name, args, self_type, locals, location)
        arg_types = define_types(args, self_type, locals)

        return receiver if receiver.dynamic?

        symbol = receiver.lookup_method(name)
        method_type = symbol.type

        unless method_type.block?
          diagnostics.undefined_method_error(receiver, name, location)

          return method_type
        end

        verify_send_arguments(receiver, method_type, args, location)

        method_type
          .initialized_return_type(receiver, [self_type, *arg_types])
      end

      def verify_send_arguments(receiver_type, type, arguments, location)
        given_count = arguments.length

        return unless verify_keyword_arguments(type, arguments)

        if type.valid_number_of_arguments?(given_count)
          verify_send_argument_types(receiver_type, type, arguments)
        else
          diagnostics.argument_count_error(
            given_count,
            type.argument_count_range,
            location
          )
        end
      end

      def verify_keyword_arguments(type, arguments)
        arguments.all? do |arg|
          next true unless arg.keyword_argument?
          next true if type.lookup_argument(arg.name).any?

          diagnostics
            .undefined_keyword_argument_error(arg.name, type, arg.location)

          false
        end
      end

      def verify_send_argument_types(receiver_type, type, arguments)
        receiver_is_module = receiver_type == @module.type

        arguments.each_with_index do |arg, index|
          # We add +1 to the index to skip the self argument.
          key = arg.keyword_argument? ? arg.name : index + 1
          exp = type.type_for_argument_or_rest(key)

          if exp.generated_trait?
            if (instance = receiver_type.type_parameter_instances[exp.name])
              exp = instance
            elsif arg.type.type_compatible?(exp) && !receiver_is_module
              receiver_type.init_type_parameter(exp.name, arg.type)
            end
          end

          verify_send_argument(arg.type, exp, arg.location)
        end
      end

      def verify_send_argument(given, expected, location)
        if expected.generated_trait? && !given.implements_trait?(expected)
          diagnostics
            .generated_trait_not_implemented_error(expected, given, location)

          return
        end

        return if given.type_compatible?(expected)

        diagnostics.type_error(expected, given, location)
      end

      def receiver_type(node, self_type, locals)
        name = node.name

        node.receiver_type =
          if node.receiver
            define_type(node.receiver, self_type, locals)
          elsif self_type.lookup_method(name).any?
            self_type
          elsif @module.globals[name].any?
            @module.type
          else
            self_type
          end
      end

      def on_raw_instruction(node, self_type, locals)
        callback = node.raw_instruction_visitor_method

        # Although we don't directly use the argument types here we still want
        # to store them in every node so we can access them later on.
        node.arguments.each { |arg| define_type(arg, self_type, locals) }

        if respond_to?(callback)
          public_send(callback, node, self_type, locals)
        else
          diagnostics.unknown_raw_instruction_error(node.name, node.location)
          typedb.nil_type
        end
      end

      def on_raw_get_toplevel(*)
        typedb.top_level
      end

      def on_raw_set_attribute(node, *)
        node.arguments[2].type
      end

      def on_raw_set_object(node, *)
        proto =
          if (proto_node = node.arguments[1])
            proto_node.type
          end

        Type::Object.new(prototype: proto)
      end

      def on_raw_integer_to_string(*)
        typedb.string_type
      end

      def on_raw_stdout_write(*)
        typedb.integer_type
      end

      def on_raw_get_true(*)
        typedb.boolean_type
      end

      alias on_raw_get_false on_raw_get_true

      def on_return(node, self_type, locals)
        if node.value
          define_type(node.value, self_type, locals)
        else
          typedb.nil_type
        end
      end

      def on_throw(node, self_type, locals)
        define_type(node.value, self_type, locals)
      end

      def on_try(node, self_type, locals)
        exp_type = define_type(node.expression, self_type, locals)
        else_type = if node.else_body
                      define_type(node.else_body, self_type, locals)
                    end

        if else_type && !else_type.type_compatible?(exp_type)
          diagnostics.type_error(exp_type, else_type, node.else_body.location)
        end

        exp_type
      end

      def on_object(node, self_type, *)
        name = node.name
        top = typedb.top_level

        proto =
          if (sym = top.lookup_attribute(Config::OBJECT_CONST)) && sym.any?
            sym.type
          end

        type = Type::Object.new(name: name, prototype: proto)

        type.define_attribute(
          Config::OBJECT_NAME_INSTANCE_ATTRIBUTE,
          typedb.string_type
        )

        define_type_parameters(node.type_parameters, type)
        store_type(type, self_type, node.location)
        define_type(node.body, type, node.body.locals)
        define_block_type_for_object(node, type)

        type
      end

      def define_block_type_for_object(node, type)
        node.block_type = Type::Block.new(
          Config::BLOCK_NAME,
          typedb.block_prototype,
          returns: node.body.type
        )

        node.block_type.define_self_argument(type)
      end

      def on_trait(node, self_type, *)
        name = node.name
        type = Type::Trait.new(name: name, prototype: trait_prototype)

        define_type_parameters(node.type_parameters, type)

        node.required_traits.each do |trait|
          trait_type = resolve_type(trait, self_type, [self_type, @module])

          type.required_traits << trait_type if trait_type.trait?
        end

        store_type(type, self_type, node.location)
        define_type(node.body, type, node.body.locals)
        define_block_type_for_object(node, type)

        type
      end

      def on_trait_implementation(node, self_type, *)
        trait = resolve_type(node.trait_name, self_type, [self_type, @module])
        object = resolve_type(node.object_name, self_type, [self_type, @module])

        loc = node.location

        define_type(node.body, object, node.body.locals)
        define_block_type_for_object(node, object)

        traits_implemented = required_traits_implemented?(object, trait, loc)
        methods_implemented = required_methods_implemented?(object, trait, loc)

        if traits_implemented && methods_implemented
          object.implemented_traits << trait
        end

        object
      end

      def required_traits_implemented?(object, trait, location)
        trait.required_traits.each do |req_trait|
          next if object.trait_implemented?(req_trait)

          diagnostics
            .uninplemented_trait_error(trait, object, req_trait, location)

          return false
        end

        true
      end

      def required_methods_implemented?(object, trait, location)
        trait.required_methods.each do |method|
          next if object.method_implemented?(method)

          diagnostics.unimplemented_method_error(method.type, object, location)

          return false
        end
      end

      def on_method(node, self_type, *)
        type = Type::Block.new(node.name, typedb.block_prototype)

        block_signature(node, type, self_type, node.body.locals)

        if node.required?
          if self_type.trait?
            self_type.define_required_method(type)
          else
            diagnostics.define_required_method_on_non_trait_error(node.location)
          end
        else
          store_type(type, self_type, node.location)
        end

        @method_bodies << DeferredMethod.new(node, self_type, node.body.locals)

        type
      end

      def process_deferred_method(method)
        node = method.ast
        body = node.body

        define_type(body, method.self_type, method.locals)

        expected_type = node.type.return_type.resolve_type(method.self_type)
        inferred_type = body.type

        return if inferred_type.type_compatible?(expected_type)

        diagnostics
          .return_type_error(expected_type, inferred_type, node.location)
      end

      def on_block(node, self_type, *)
        type = Type::Block.new(Config::BLOCK_NAME, typedb.block_prototype)

        block_signature(node, type, self_type, node.body.locals)
        define_type(node.body, self_type, node.body.locals)

        rtype = node.body.type
        exp = type.resolve_type(self_type)

        type.returns = rtype if type.returns.dynamic?

        unless rtype.type_compatible?(exp)
          diagnostics.return_type_error(exp, rtype, node.location)
        end

        type
      end

      def on_define_variable(node, self_type, locals)
        callback = node.variable.define_variable_visitor_method
        vtype = define_type(node.value, self_type, locals)

        public_send(callback, node, self_type, vtype, locals)

        node.variable.type = vtype
      end

      def on_define_constant(node, self_type, value_type, *)
        store_type(value_type, self_type, node.location, node.variable.name)
      end

      def on_define_attribute(node, self_type, value_type, *)
        self_type.define_attribute(node.variable.name, value_type)
      end

      def on_define_local(node, _, value_type, locals)
        locals.define(node.variable.name, value_type, node.mutable?)
      end

      def on_reassign_variable(node, self_type, locals)
        callback = node.variable.reassign_variable_visitor_method
        vtype = define_type(node.value, self_type, locals)

        public_send(callback, node, self_type, vtype, locals)

        node.variable.type = vtype
      end

      def on_reassign_attribute(node, self_type, value_type, *)
        name = node.variable.name
        symbol = self_type.lookup_attribute(name)
        existing_type = symbol.type

        if symbol.nil?
          diagnostics.reassign_undefined_attribute_error(name, node.location)
          return existing_type
        end

        return if value_type.type_compatible?(existing_type)

        diagnostics.type_error(existing_type, value_type, node.value.location)
      end

      def on_reassign_local(node, _, value_type, locals)
        name = node.variable.name
        local = locals[name]
        existing_type = local.type

        if local.nil?
          diagnostics.reassign_undefined_local_error(name, node.location)
          return existing_type
        end

        return if value_type.type_compatible?(existing_type)

        diagnostics.type_error(existing_type, value_type, node.value.location)
      end

      def block_signature(node, type, self_type, locals)
        define_type_parameters(node.type_parameters, type)
        define_arguments(node.arguments, type, self_type, locals)
        define_return_type(node, type, self_type)
        define_throw_type(node, type, self_type)
      end

      def define_arguments(arguments, block_type, self_type, locals)
        block_type.define_self_argument(self_type)

        arguments.each do |arg|
          val_type = type_for_argument_value(arg, self_type, locals)
          def_type = defined_type_for_argument(arg, block_type, self_type)

          # If both an explicit type and default value are given we need to make
          # sure the two are compatible.
          if argument_types_incompatible?(def_type, val_type)
            diagnostics.type_error(def_type, val_type, arg.default.location)
          end

          arg_name = arg.name
          arg_type = def_type || val_type || Type::Dynamic.new

          if arg.default
            block_type.define_argument(arg_name, arg_type)
          elsif arg.rest?
            block_type.define_rest_argument(arg_name, arg_type)
          else
            block_type.define_required_argument(arg_name, arg_type)
          end

          arg.type = arg_type

          locals.define(arg_name, arg_type)
        end
      end

      def define_return_type(node, block_type, self_type)
        rnode = node.returns

        unless rnode
          block_type.returns = Type::Dynamic.new
          return
        end

        if rnode.self_type?
          block_type.returns = Type::SelfType.new
          return
        end

        block_type.returns = wrap_optional_type(
          rnode,
          resolve_type(rnode, self_type, [block_type, self_type, @module])
        )
      end

      def define_throw_type(node, block_type, self_type)
        return unless node.throws

        block_type.throws = wrap_optional_type(
          node.returns,
          resolve_type(node.throws, self_type, [block_type, self_type, @module])
        )
      end

      def type_for_argument_value(arg, self_type, locals)
        define_type(arg.default, self_type, locals) if arg.default
      end

      def defined_type_for_argument(arg, block_type, self_type)
        return unless arg.type

        wrap_optional_type(
          arg.type,
          resolve_type(arg.type, self_type, [block_type, self_type, @module])
        )
      end

      def argument_types_incompatible?(defined_type, value_type)
        defined_type && value_type && !defined_type.type_compatible?(value_type)
      end

      def store_type(type, self_type, location, name = type.name)
        self_type.define_attribute(name, type)

        if Config::RESERVED_CONSTANTS.include?(name)
          diagnostics.redefine_reserved_constant_error(name, location)
        end

        return if type.block? || !module_scope?(self_type)

        @module.globals.define(name, type)
      end

      def module_scope?(self_type)
        self_type == @module.type
      end

      def wrap_optional_type(node, type)
        node.optional? ? Type::Optional.new(type) : type
      end

      def trait_prototype
        typedb.top_level.lookup_attribute(Config::TRAIT_CONST).type
      end

      def define_type_parameters(arguments, type)
        proto = trait_prototype

        arguments.each do |arg_node|
          required_traits = arg_node.required_traits.map do |node|
            resolve_type(node, type, [type, self.module])
          end

          trait = Type::Trait
            .new(name: arg_node.name, prototype: proto, generated: true)

          trait.required_traits.merge(required_traits)
          type.define_type_parameter(trait.name, trait)
        end
      end

      def resolve_type(node, self_type, sources)
        return Type::SelfType.new if node.self_type?

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

        Type::Dynamic.new(name)
      end

      def inspect
        # The default inspect is very slow, slowing down the rendering of any
        # runtime errors.
        '#<Pass::DefineTypes>'
      end
    end
  end
end
