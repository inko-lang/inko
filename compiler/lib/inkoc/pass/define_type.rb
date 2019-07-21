# frozen_string_literal: true

module Inkoc
  module Pass
    # rubocop: disable Metrics/ClassLength
    class DefineType
      include VisitorMethods
      include TypePass

      DeferredMethod = Struct.new(:node, :scope)

      def initialize(mod, state)
        super

        @constant_resolver = ConstantResolver.new(diagnostics)
        @deferred_methods = []
      end

      def process_deferred_methods
        @deferred_methods.each do |method|
          on_deferred_method(method.node, method.scope)
        end
      end

      def define_type_instance(node, scope, *extra)
        type = define_type(node, scope, *extra)

        unless type.type_instance?
          type = type.new_instance
          node.type = type
        end

        type
      end

      def on_module_body(node, scope)
        type = define_type(node, scope)

        process_deferred_methods

        type
      end

      def on_integer(*)
        typedb.integer_type.new_instance
      end

      def on_float(*)
        typedb.float_type.new_instance
      end

      def on_string(*)
        typedb.string_type.new_instance
      end

      def on_constant(node, scope)
        @constant_resolver.resolve(node, scope)
      end

      def on_type_name_reference(node, scope)
        type = define_type(node.name, scope)

        return type if type.error?

        if same_type_parameters?(node, type)
          wrap_optional_type(node, type)
        else
          TypeSystem::Error.new
        end
      end

      def same_type_parameters?(node, type)
        node_names = node.type_parameters.map(&:type_name)
        type_names = type.type_parameters.map(&:name)

        if node_names == type_names
          true
        else
          diagnostics.invalid_type_parameters(type, node_names, node.location)
          false
        end
      end

      def on_block_type(node, scope)
        proto = @state.typedb.block_type
        type =
          if node.lambda_type?
            TypeSystem::Block.lambda(proto)
          else
            TypeSystem::Block.closure(proto)
          end

        type.self_type = scope.self_type
        type.define_call_method

        arg_types = node.arguments.map do |arg|
          define_type_instance(arg, scope)
        end

        type.define_arguments(arg_types)

        if node.returns
          type.return_type = define_type_instance(node.returns, scope)
        end

        if node.throws
          type.throw_type = define_type_instance(node.throws, scope)
        end

        wrap_optional_type(node, type)
      end
      alias on_lambda_type on_block_type

      def on_self_type_with_late_binding(node, _)
        wrap_optional_type(node, TypeSystem::SelfType.new)
      end

      def on_self_type(node, scope)
        self_type = scope.self_type

        # When "Self" translates to a generic type, e.g. Array!(T), we want to
        # return a type in the form of `Array!(T -> T)`, and not just `Array`.
        # This ensures that any arguments passed to a method returning "Self"
        # can properly initialise the type.
        type_arguments =
          self_type.generic_type? ? self_type.type_parameters.to_a : []

        wrap_optional_type(node, self_type.new_instance(type_arguments))
      end

      def on_dynamic_type(node, _)
        wrap_optional_type(node, TypeSystem::Dynamic.new)
      end

      def on_void_type(node, _)
        wrap_optional_type(node, TypeSystem::Void.new)
      end

      def on_type_name(node, scope)
        type = define_type(node.name, scope)

        return type if type.error?
        return wrap_optional_type(node, type) unless type.generic_type?

        # When our type is a generic type we need to initialise it according to
        # the passed type parameters.
        type_arguments = node
          .type_parameters
          .zip(type.type_parameters)
          .map do |param_node, param|
            param_instance = define_type_instance(param_node, scope)

            if param && !param_instance.type_compatible?(param, @state)
              return diagnostics
                  .type_error(param, param_instance, param_node.location)
            end

            param_instance
          end

        num_given = type_arguments.length
        num_expected = type.type_parameters.length

        if num_given != num_expected
          return diagnostics.type_parameter_count_error(
            num_given,
            num_expected,
            node.location
          )
        end

        # Simply referencing a constant should not lead to it being initialised,
        # unless there are any type parameters to initialise.
        wrap_optional_type(
          node,
          type.new_instance_for_reference(type_arguments)
        )
      end

      def on_attribute(node, scope)
        name = node.name
        symbol = scope.self_type.lookup_attribute(name)

        if symbol.nil?
          diagnostics
            .undefined_attribute_error(scope.self_type, name, node.location)

          TypeSystem::Error.new
        else
          symbol.type
        end
      end

      def on_identifier(node, scope)
        name = node.name
        loc = node.location
        self_type = scope.self_type

        if (depth_sym = scope.depth_and_symbol_for_local(name))
          node.depth = depth_sym[0]
          node.symbol = depth_sym[1]

          remap_send_return_type(node.symbol.type, scope)
        elsif self_type.responds_to_message?(name)
          identifier_send(node, scope.self_type, name, scope)
        elsif scope.module_type.responds_to_message?(name)
          identifier_send(node, scope.module_type, name, scope)
        elsif (global = @module.lookup_global(name))
          global
        else
          diagnostics.undefined_method_error(self_type, name, loc)
          TypeSystem::Error.new
        end
      end

      def identifier_send(node, source, name, scope)
        node.block_type = source.lookup_method(name).type
        return_type = source.message_return_type(name, scope.self_type)

        remap_send_return_type(return_type, scope)
      end

      def on_self(_, scope)
        scope.self_type.new_instance
      end

      def on_send(node, scope)
        node.receiver_type = source = type_of_receiver(node, scope)

        if source.dynamic?
          send_to_dynamic_type(node, scope)
        elsif source.error?
          source
        elsif source.optional?
          send_to_optional_type(node, source, scope)
        else
          send_to_known_type(node, source, scope)
        end
      end

      def send_to_dynamic_type(node, scope)
        define_types(node.arguments, scope)

        TypeSystem::Dynamic.new
      end

      def send_to_optional_type(node, source, scope)
        nil_type = typedb.nil_type
        rtype = send_to_known_type(node, source, scope)
        name = node.name

        return rtype if rtype.error?

        if (nil_impl = nil_type.lookup_method(name)) && nil_impl.any?
          rec_impl = source.lookup_method(name).type

          # Only if the receiver and Nil implement the message in a compatible
          # way can we return the message's return type directly.
          if rec_impl.type_compatible?(nil_impl.type, @state)
            rtype
          else
            # Nil and the receiver have different implementations of the same
            # message. For example, type T implements "foo -> Integer" while Nil
            # implements it as "foo -> String". In this case the compiler can
            # not make a reasonable guess as to what the type will be, so we
            # just error instead.
            diagnostics.incompatible_optional_method(
              source,
              nil_type,
              name,
              node.location
            )
          end
        else
          TypeSystem::Optional.wrap(rtype)
        end
      end

      def send_to_known_type(node, source, scope)
        name = node.name
        method = source.lookup_method(name).type_or_else do
          unknown_method = source.lookup_unknown_message(@state)

          if unknown_method.nil?
            return diagnostics
                .undefined_method_error(source, name, node.location)
          end

          # If the method is not defined, but the receiver _does_ implement
          # "unknown_message", just return the type of that implementation.
          return unknown_method.type.resolved_return_type(source)
        end

        unless verify_method_bounds(source, method, node.location)
          return TypeSystem::Error.new
        end

        exp_args = method.argument_count_range

        unless exp_args.cover?(node.arguments.length)
          return diagnostics.argument_count_error(
            node.arguments.length,
            exp_args,
            node.location
          )
        end

        method = initialize_method_for_send(node, method, scope)

        return method if method.error?

        verify_argument_types_and_initialize(node, source, method, scope)
      end

      def verify_method_bounds(receiver, method, loc)
        method.method_bounds.all? do |bound|
          param = receiver.lookup_type_parameter(bound.name)
          instance = receiver.lookup_type_parameter_instance(param)

          if !instance || instance.type_compatible?(bound, @state)
            true
          else
            diagnostics
              .method_requirement_error(receiver, method, instance, bound, loc)

            false
          end
        end
      end

      # rubocop: disable Metrics/CyclomaticComplexity
      # rubocop: disable Metrics/PerceivedComplexity
      # rubocop: disable Metrics/BlockLength
      # rubocop: disable Metrics/AbcSize
      def verify_argument_types_and_initialize(node, source, method, scope)
        node.arguments.each_with_index do |arg_node, index|
          rest = false

          if arg_node.keyword_argument?
            keyword_type = method.keyword_argument_type(arg_node.name, source)

            unless keyword_type
              return diagnostics.undefined_keyword_argument_error(
                arg_node.name,
                source,
                method,
                arg_node.location
              )
            end

            exp_arg = keyword_type
          else
            exp_arg, rest = method.argument_type_at(index, source)
          end

          exp_arg = exp_arg.resolve_type_parameters(source, method)

          given_arg =
            if arg_node.closure? && exp_arg.block?
              # When passing a closure to a closure we want to infer the
              # arguments of our given closure according to the arguments of the
              # expected closure.
              #
              # Before we do this, we create a copy of the expected closure and
              # make sure any instance type parameters are initialised. This
              # ensures that if the expected argument of a closure is "T", we
              # use any corresponding type parameter instances if available,
              # instead of just using "T" as-is.
              #
              # In other words, if the expected block is defined like this:
              #
              #     do (T)
              #
              # And our given block is defined like this:
              #
              #     do (thing) { ... }
              #
              # Then "thing" will be whatever instance is bound to type
              # parameter "T", or "T" itself is no instance was bound.
              exp_arg =
                exp_arg.with_type_parameter_instances_from([source, method])

              # When passing a block without a signature (e.g. `foo { 10 }`) we
              # want to infer this as a lambda, if the expected block is also a
              # lambda. This allows one to write code such as the following:
              #
              #     process.spawn {
              #       ...
              #     }
              #
              # Instead of having to write this:
              #
              #     process.spawn lambda {
              #       ...
              #     }
              if arg_node.block_without_signature? && exp_arg.lambda?
                arg_node.infer_as_lambda
              end

              define_type(arg_node, scope, exp_arg)
            else
              define_type(arg_node, scope)
            end

          # When the expected argument is a rest type we need to compare
          # with/initialise the type of the individual rest values. For example,
          # for rest argument `*foo: X` the actual type of `foo` is `Array!(X)`,
          # but we want to compare with/initialise _just_ `X`.
          compare_with = rest ? type_of_rest_argument_value(exp_arg) : exp_arg

          unless given_arg.type_compatible?(compare_with, @state)
            return diagnostics.type_error(exp_arg, given_arg, arg_node.location)
          end

          compare_with.initialize_as(given_arg, method, source)
        end

        node.block_type = method
        return_type = method.resolved_return_type(source)

        remap_send_return_type(return_type, scope)
      end
      # rubocop: enable Metrics/AbcSize
      # rubocop: enable Metrics/BlockLength
      # rubocop: enable Metrics/PerceivedComplexity
      # rubocop: enable Metrics/CyclomaticComplexity

      def initialize_method_for_send(node, method, scope)
        given = node.type_arguments.length
        max = method.type_parameters.length

        if given > max
          return diagnostics.too_many_type_parameters(max, given, node.location)
        end

        type_args = node.type_arguments.map do |type_arg_node|
          define_type_instance(type_arg_node, scope)
        end

        method.new_instance_for_send(type_args)
      end

      def remap_send_return_type(type, scope)
        if (surrounding_method = scope.enclosing_method)
          type.remap_using_method_bounds(surrounding_method)
        else
          type
        end
      end

      def type_of_receiver(node, scope)
        if node.receiver
          receiver_type_for_send_with_receiver(node, scope)
        elsif scope.self_type.responds_to_message?(node.name)
          scope.self_type
        else
          scope.module_type
        end
      end

      def receiver_type_for_send_with_receiver(node, scope)
        if node.name == Config::NEW_MESSAGE
          define_type_instance(node.receiver, scope)
        elsif node.hash_map_literal?
          @state.module(Config::HASH_MAP_MODULE).type
        else
          define_type(node.receiver, scope)
        end
      end

      def type_of_rest_argument_value(type)
        param = type
          .lookup_type_parameter(Config::ARRAY_TYPE_PARAMETER)

        type.lookup_type_parameter_instance(param)
      end

      def on_body(node, scope)
        type =
          define_types(node.expressions, scope).last ||
          typedb.nil_type.new_instance

        block_type = scope.block_type

        block_type.return_type = type if block_type.infer_return_type
        expected_type =
          block_type.return_type.resolve_self_type(scope.self_type)

        if !type.void? && !type.type_compatible?(expected_type, @state)
          loc = node.location_of_last_expression

          diagnostics.return_type_error(expected_type, type, loc)
        end

        type
      end

      def on_return(node, scope)
        rtype =
          if node.value
            define_type(node.value, scope)
          else
            typedb.nil_type.new_instance
          end

        if (method = scope.enclosing_method)
          expected = method.return_type.resolve_self_type(scope.self_type)

          unless rtype.type_compatible?(expected, @state)
            diagnostics
              .return_type_error(expected, rtype, node.value_location)
          end
        else
          diagnostics.return_outside_of_method_error(node.location)
        end

        # A "return" statement itself will never return a value. For example,
        # `let x = return 10` would never assign a value to `x`.
        TypeSystem::Void.new
      end

      def on_try(node, scope)
        define_type(node.expression, scope)

        if node.empty_else?
          on_try_without_else(node, scope)
        else
          on_try_with_else(node, scope)
        end
      end

      def on_try_without_else(node, scope)
        ret_type = node.expression.type
        curr_block = scope.block_type

        if (throw_type = node.throw_type)
          curr_block.throw_type = throw_type if curr_block.infer_throw_type?
        else
          diagnostics.redundant_try_warning(node.location)
        end

        ret_type
      end

      def on_try_with_else(node, scope)
        try_type = node.expression.type
        throw_type = node.throw_type || TypeSystem::Dynamic.new

        node.else_block_type = TypeSystem::Block.new(
          name: Config::ELSE_BLOCK_NAME,
          prototype: @state.typedb.block_type
        )

        else_scope = TypeScope.new(
          scope.self_type,
          node.else_block_type,
          @module,
          locals: node.else_body.locals,
          parent: scope
        )

        else_scope.define_receiver_type

        if (else_arg_name = node.else_argument_name)
          node.else_block_type.arguments.define(else_arg_name, throw_type)
          else_scope.locals.define(else_arg_name, throw_type)
        end

        else_type = define_type(node.else_body, else_scope)

        # If "try" returns X and "else" returns Nil then we want to infer the
        # type to a `?X`.
        if infer_try_as_optional?(try_type, else_type)
          node.type = try_type = TypeSystem::Optional.wrap(try_type)
        end

        if else_type.type_compatible?(try_type, @state)
          try_type
        else
          diagnostics.type_error(try_type, else_type, node.else_body.location)
        end
      end

      def infer_try_as_optional?(try_type, else_type)
        nil_type = typedb.nil_type

        if try_type.type_instance_of?(nil_type) &&
           else_type.type_instance_of?(nil_type)
          return false
        end

        !try_type.optional? && else_type.type_instance_of?(nil_type)
      end

      def on_throw(node, scope)
        type = define_type(node.value, scope)

        scope.block_type.throw_type = type if scope.block_type.infer_throw_type?

        TypeSystem::Void.new
      end

      def on_object(node, scope)
        type = typedb.new_object_type(node.name)

        define_object_name_attribute(type)
        define_named_type(node, type, scope)
      end

      def on_trait(node, scope)
        if (existing = scope.lookup_type(node.name))
          extend_trait(existing, node, scope)
        else
          type = typedb.new_trait_type(node.name)

          define_object_name_attribute(type)
          define_required_traits(node, type, scope)
          define_named_type(node, type, scope)
        end
      end

      def extend_trait(trait, node, scope)
        unless trait.empty?
          return diagnostics.extend_trait_error(trait, node.location)
        end

        return TypeSystem::Error.new unless same_type_parameters?(node, trait)

        node.redefines = true

        define_required_traits(node, trait, scope)

        body_type = TypeSystem::Block.closure(typedb.block_type)

        body_scope = TypeScope
          .new(trait, body_type, @module, locals: node.body.locals)

        body_scope.define_receiver_type

        node.block_type = body_type

        define_type(node.body, body_scope)

        trait
      end

      def define_object_name_attribute(type)
        type.define_attribute(
          Config::OBJECT_NAME_INSTANCE_ATTRIBUTE,
          typedb.string_type.new_instance
        )
      end

      def define_required_traits(node, trait, scope)
        node.required_traits.each do |req_node|
          req = define_type_instance(req_node, scope)

          trait.add_required_trait(req) unless req.error?
        end
      end

      def define_named_type(node, new_type, scope)
        body_type = TypeSystem::Block.closure(typedb.block_type)

        body_scope = TypeScope
          .new(new_type, body_type, @module, locals: node.body.locals)

        body_scope.define_receiver_type

        node.block_type = body_type

        define_types(node.type_parameters, body_scope)
        store_type(new_type, scope, node.location)
        define_type(node.body, body_scope)

        new_type
      end

      def on_reopen_object(node, scope)
        type = on_type_name_reference(node.name, scope)

        return type if type.error?

        unless type.object?
          return diagnostics.reopen_invalid_object_error(
            node.name.qualified_name,
            node.location
          )
        end

        block_type = TypeSystem::Block.closure(typedb.block_type)

        new_scope = TypeScope
          .new(type, block_type, @module, locals: node.body.locals)

        new_scope.define_receiver_type

        node.block_type = block_type

        define_type(node.body, new_scope)

        type
      end

      def on_trait_implementation(node, scope)
        object = on_type_name_reference(node.object_name, scope)

        return object if object.error?

        # The trait name has to be looked up in the context of the
        # implementation. This ensures that a Self type refers to the type
        # that the trait is implemented for, instead of referring to the type of
        # the outer scope.
        impl_block = TypeSystem::Block.closure(typedb.block_type)
        impl_scope = TypeScope
          .new(object, impl_block, @module, locals: node.body.locals)

        impl_scope.define_receiver_type

        trait = define_type(node.trait_name, impl_scope)

        return trait if trait.error?

        object.implement_trait(trait)

        node.block_type = impl_block

        define_type(node.body, impl_scope)

        if trait_requirements_met?(object, trait, node.location)
          trait
        else
          object.remove_trait_implementation(trait)

          TypeSystem::Error.new
        end
      end

      def trait_requirements_met?(object, trait, location)
        required_traits_implemented?(object, trait, location) &&
          required_methods_implemented?(object, trait, location)
      end

      def required_traits_implemented?(object, trait, location)
        trait.required_trait_types.all? do |required|
          if object.implements_trait?(required)
            true
          else
            diagnostics
              .uninplemented_trait_error(trait, object, required, location)

            false
          end
        end
      end

      def required_methods_implemented?(object, trait, location)
        trait.required_methods.all? do |required|
          req_method = required.type.with_type_parameter_instances_from([trait])

          if object.implements_method?(req_method, @state)
            true
          else
            diagnostics
              .unimplemented_method_error(req_method, object, location)

            false
          end
        end
      end

      def on_method(node, scope)
        type = TypeSystem::Block.named_method(node.name, typedb.block_type)

        new_scope = TypeScope.new(
          scope.self_type.new_instance,
          type,
          @module,
          locals: node.body.locals
        )

        define_method_bounds(node, new_scope)
        define_block_signature(node, new_scope)

        store_type(type, scope, node.location)

        @deferred_methods << DeferredMethod.new(node, new_scope)

        type
      end

      def on_required_method(node, scope)
        type = TypeSystem::Block.named_method(node.name, typedb.block_type)

        new_scope = TypeScope
          .new(scope.self_type, type, @module, locals: node.body.locals)

        define_block_signature(node, new_scope)

        if scope.self_type.trait?
          scope.self_type.define_required_method(type)
        else
          diagnostics.define_required_method_on_non_trait_error(node.location)
        end

        type
      end

      def on_deferred_method(node, scope)
        define_type(node.body, scope)
      end

      def store_type(type, scope, location)
        scope.self_type.define_attribute(type.name, type)

        store_type_as_global(type.name, type, scope, location)
      end

      def store_type_as_global(name, type, scope, location)
        if Config::RESERVED_CONSTANTS.include?(name)
          diagnostics.redefine_reserved_constant_error(name, location)
        elsif scope.module_scope?
          @module.globals.define(name, type)
        end
      end

      def on_block(node, scope, expected_block = nil)
        block_type = TypeSystem::Block.closure(typedb.block_type)
        locals = node.body.locals

        new_scope = TypeScope.new(
          scope.self_type,
          block_type,
          @module,
          locals: locals,
          parent: scope
        )

        define_block_signature(node, new_scope, expected_block)
        define_type(node.body, new_scope)

        block_type
      end

      def on_lambda(node, scope, expected_block = nil)
        block_type = TypeSystem::Block.lambda(typedb.block_type)
        new_scope = TypeScope.new(
          @module.type,
          block_type,
          @module,
          locals: node.body.locals,
          enclosing_method: scope.enclosing_method
        )

        define_block_signature(node, new_scope, expected_block)
        define_type(node.body, new_scope)

        block_type
      end

      def on_define_variable(node, scope)
        vtype = define_type(node.value, scope)
        callback = node.variable.define_variable_visitor_method

        public_send(callback, node.variable, vtype, scope, node.mutable?)
      end

      def on_define_variable_with_explicit_type(node, scope)
        vtype = define_type(node.value, scope)
        exp_type = define_type_instance(node.value_type, scope)
        callback = node.variable.define_variable_visitor_method

        vtype =
          if vtype.type_compatible?(exp_type, @state)
            exp_type
          else
            diagnostics.type_error(exp_type, vtype, node.location)
          end

        public_send(callback, node.variable, vtype, scope, node.mutable?)
      end

      def on_define_local(node, value_type, scope, mutable = false)
        name = node.name

        if scope.locals.defined?(name)
          value_type = diagnostics
            .redefine_existing_local_error(name, node.location)
        else
          scope.locals.define(name, value_type, mutable)
        end

        value_type
      end

      def on_define_attribute(node, scope)
        name = node.name
        vtype = define_type_instance(node.value_type, scope)

        if scope.self_type.lookup_attribute(name).any?
          diagnostics
            .redefine_existing_attribute_error(name, node.location)
        else
          scope.self_type.define_attribute(name, vtype, true)

          vtype
        end
      end

      def on_define_constant(node, value_type, scope, _)
        name = node.name

        if scope.self_type.lookup_attribute(name).any?
          value_type = diagnostics
            .redefine_existing_constant_error(name, node.location)
        else
          scope.self_type.define_attribute(name, value_type)
        end

        store_type_as_global(name, value_type, scope, node.location)

        value_type
      end

      def on_reassign_variable(node, scope)
        callback = node.variable.reassign_variable_visitor_method
        value_type = define_type(node.value, scope)

        public_send(callback, node.variable, value_type, scope)
      end

      def on_reassign_local(node, value_type, scope)
        name = node.name
        _, existing = scope.locals.lookup_with_parent(name)

        unless existing.any?
          return diagnostics.reassign_undefined_local_error(name, node.location)
        end

        unless existing.mutable?
          diagnostics.reassign_immutable_local_error(name, node.location)
          return existing.type
        end

        unless value_type.type_compatible?(existing.type, @state)
          diagnostics.type_error(existing.type, value_type, node.location)
        end

        existing.type
      end

      def on_reassign_attribute(node, value_type, scope)
        name = node.name
        existing = scope.self_type.lookup_attribute(name)

        unless existing.any?
          return diagnostics
              .reassign_undefined_attribute_error(name, node.location)
        end

        unless existing.mutable?
          diagnostics.reassign_immutable_attribute_error(name, node.location)
          return existing.type
        end

        unless value_type.type_compatible?(existing.type, @state)
          diagnostics.type_error(existing.type, value_type, node.location)
        end

        existing.type
      end

      def on_define_argument(arg_node, scope, default_type = nil)
        block_type = scope.block_type
        name = arg_node.name
        mutable = arg_node.mutable?

        vtype = type_for_argument_value(arg_node, scope)
        def_type = defined_type_for_argument(arg_node, scope)
        arg_type = determine_argument_type(
          arg_node,
          def_type,
          vtype,
          scope.block_type,
          default_type
        )

        symbol =
          if arg_node.default
            block_type.define_optional_argument(name, arg_type, mutable)
          elsif arg_node.rest?
            block_type.define_rest_argument(
              name,
              @state.typedb.new_array_of_type(arg_type),
              mutable
            )
          else
            block_type.define_required_argument(name, arg_type, mutable)
          end

        scope.locals.add_symbol(symbol)

        arg_type
      end

      def on_define_type_parameter(node, scope)
        traits = define_types(node.required_traits, scope)

        scope.self_type.define_type_parameter(node.name, traits)
      end

      def on_keyword_argument(node, scope)
        define_type(node.value, scope)
      end

      def on_type_cast(node, scope)
        to_cast = define_type(node.expression, scope)
        cast_to = define_type_instance(node.cast_to, scope)

        if to_cast.cast_to?(cast_to, @state)
          cast_to
        else
          diagnostics.invalid_cast_error(to_cast, cast_to, node.location)
        end
      end

      def on_global(node, _)
        if (symbol = @module.globals[node.name]) && symbol.any?
          symbol.type
        else
          diagnostics.undefined_constant_error(node.name, node.location)
        end
      end

      def on_dereference(node, scope)
        type = define_type(node.expression, scope)

        if type.dereference?
          type.dereferenced_type
        else
          diagnostics.dereference_error(type, node.location)
          type
        end
      end

      def on_raw_instruction(node, scope)
        callback = node.raw_instruction_visitor_method

        define_types(node.arguments, scope)

        if respond_to?(callback)
          public_send(callback, node, scope)
        else
          diagnostics.unknown_raw_instruction_error(node.name, node.location)

          TypeSystem::Error.new
        end
      end

      def on_raw_get_toplevel(*)
        typedb.top_level.new_instance
      end

      def on_raw_set_prototype(node, _)
        node.arguments.fetch(1).type
      end

      def on_raw_set_attribute(node, *)
        node.arguments.fetch(2).type
      end

      def on_raw_set_attribute_to_object(*)
        typedb.new_empty_object.new_instance
      end

      def on_raw_get_attribute(node, *)
        object = node.arguments.fetch(0).type
        name = node.arguments.fetch(1)

        if name.string?
          object.lookup_attribute(name.value).type
        else
          TypeSystem::Dynamic.new
        end
      end

      def on_raw_set_object(node, *)
        if (proto = node.arguments[1]&.type)
          proto = proto.type if proto.optional?

          proto.new_instance
        else
          typedb.new_empty_object
        end
      end

      def on_raw_object_equals(*)
        typedb.boolean_type.new_instance
      end

      def on_raw_copy_blocks(*)
        TypeSystem::Void.new
      end

      def on_raw_integer_to_string(*)
        typedb.string_type.new_instance
      end

      def on_raw_integer_to_float(*)
        typedb.float_type.new_instance
      end

      def on_raw_integer_add(*)
        typedb.integer_type.new_instance
      end

      def on_raw_integer_div(*)
        typedb.integer_type.new_instance
      end

      def on_raw_integer_mul(*)
        typedb.integer_type.new_instance
      end

      def on_raw_integer_sub(*)
        typedb.integer_type.new_instance
      end

      def on_raw_integer_mod(*)
        typedb.integer_type.new_instance
      end

      def on_raw_integer_bitwise_and(*)
        typedb.integer_type.new_instance
      end

      def on_raw_integer_bitwise_or(*)
        typedb.integer_type.new_instance
      end

      def on_raw_integer_bitwise_xor(*)
        typedb.integer_type.new_instance
      end

      def on_raw_integer_shift_left(*)
        typedb.integer_type.new_instance
      end

      def on_raw_integer_shift_right(*)
        typedb.integer_type.new_instance
      end

      def on_raw_integer_smaller(*)
        typedb.boolean_type.new_instance
      end

      def on_raw_integer_greater(*)
        typedb.boolean_type.new_instance
      end

      def on_raw_integer_equals(*)
        typedb.boolean_type.new_instance
      end

      def on_raw_integer_greater_or_equal(*)
        typedb.boolean_type.new_instance
      end

      def on_raw_integer_smaller_or_equal(*)
        typedb.boolean_type.new_instance
      end

      def on_raw_float_to_string(*)
        typedb.string_type.new_instance
      end

      def on_raw_float_to_integer(*)
        typedb.integer_type.new_instance
      end

      def on_raw_float_add(*)
        typedb.float_type.new_instance
      end

      def on_raw_float_div(*)
        typedb.float_type.new_instance
      end

      def on_raw_float_mul(*)
        typedb.float_type.new_instance
      end

      def on_raw_float_sub(*)
        typedb.float_type.new_instance
      end

      def on_raw_float_mod(*)
        typedb.float_type.new_instance
      end

      def on_raw_float_smaller(*)
        typedb.boolean_type.new_instance
      end

      def on_raw_float_greater(*)
        typedb.boolean_type.new_instance
      end

      def on_raw_float_equals(*)
        typedb.boolean_type.new_instance
      end

      def on_raw_float_greater_or_equal(*)
        typedb.boolean_type.new_instance
      end

      def on_raw_float_smaller_or_equal(*)
        typedb.boolean_type.new_instance
      end

      def on_raw_float_is_nan(*)
        typedb.boolean_type.new_instance
      end

      def on_raw_float_is_infinite(*)
        typedb.boolean_type.new_instance
      end

      def on_raw_float_ceil(*)
        typedb.float_type.new_instance
      end

      def on_raw_float_floor(*)
        typedb.float_type.new_instance
      end

      def on_raw_float_round(*)
        typedb.float_type.new_instance
      end

      def on_raw_stdout_write(*)
        typedb.integer_type.new_instance
      end

      def on_raw_get_true(*)
        typedb.true_type.new_instance
      end

      def on_raw_get_false(*)
        typedb.false_type.new_instance
      end

      def on_raw_get_nil(*)
        typedb.nil_type.new_instance
      end

      def on_raw_run_block(*)
        TypeSystem::Dynamic.new
      end

      def on_raw_get_string_prototype(*)
        typedb.string_type.new_instance
      end

      def on_raw_get_integer_prototype(*)
        typedb.integer_type.new_instance
      end

      def on_raw_get_float_prototype(*)
        typedb.float_type.new_instance
      end

      def on_raw_get_object_prototype(*)
        typedb.object_type.new_instance
      end

      def on_raw_get_array_prototype(*)
        typedb.array_type.new_instance
      end

      def on_raw_get_block_prototype(*)
        typedb.block_type.new_instance
      end

      def optional_array_element_value(array)
        param = array.lookup_type_parameter(Config::ARRAY_TYPE_PARAMETER)
        type = array.lookup_type_parameter_instance(param) || param

        TypeSystem::Optional.wrap(type)
      end

      def on_raw_array_length(*)
        typedb.integer_type.new_instance
      end

      def on_raw_array_at(node, _)
        optional_array_element_value(node.arguments.fetch(0).type)
      end

      def on_raw_array_set(node, _)
        node.arguments.fetch(2).type
      end

      def on_raw_array_clear(*)
        TypeSystem::Void.new
      end

      def on_raw_array_remove(node, _)
        optional_array_element_value(node.arguments.fetch(0).type)
      end

      def on_raw_time_monotonic(*)
        typedb.float_type.new_instance
      end

      def on_raw_time_system(*)
        typedb.float_type.new_instance
      end

      def on_raw_time_system_offset(*)
        typedb.integer_type.new_instance
      end

      def on_raw_time_system_dst(*)
        typedb.boolean_type.new_instance
      end

      def on_raw_string_to_upper(*)
        typedb.string_type.new_instance
      end

      def on_raw_string_to_lower(*)
        typedb.string_type.new_instance
      end

      def on_raw_string_to_byte_array(*)
        TypeSystem::Dynamic.new
      end

      def on_raw_string_size(*)
        typedb.integer_type.new_instance
      end

      def on_raw_string_length(*)
        typedb.integer_type.new_instance
      end

      def on_raw_string_equals(*)
        typedb.boolean_type.new_instance
      end

      def on_raw_string_concat(*)
        typedb.string_type.new_instance
      end

      def on_raw_string_slice(*)
        typedb.string_type.new_instance
      end

      def on_raw_stdin_read(*)
        typedb.integer_type.new_instance
      end

      def on_raw_stderr_write(*)
        typedb.integer_type.new_instance
      end

      def on_raw_process_spawn(node, _)
        node.arguments.fetch(0).type.new_instance
      end

      def on_raw_process_send_message(node, _)
        node.arguments.fetch(1).type
      end

      def on_raw_process_receive_message(*)
        TypeSystem::Dynamic.new
      end

      def on_raw_process_current(node, _)
        node.arguments.fetch(0).type.new_instance
      end

      def on_raw_process_suspend_current(*)
        TypeSystem::Void.new
      end

      def on_raw_process_terminate_current(*)
        TypeSystem::Void.new
      end

      def on_raw_remove_attribute(node, _)
        object = node.arguments.fetch(0).type
        name = node.arguments.fetch(1)

        if name.string?
          object.lookup_attribute(name.value).type
        else
          TypeSystem::Dynamic.new
        end
      end

      def on_raw_get_prototype(*)
        typedb.object_type.new_instance
      end

      def on_raw_get_attribute_names(*)
        typedb.new_array_of_type(typedb.string_type.new_instance)
      end

      def on_raw_attribute_exists(*)
        typedb.boolean_type.new_instance
      end

      def on_raw_file_flush(*)
        typedb.nil_type.new_instance
      end

      def on_raw_stdout_flush(*)
        typedb.nil_type.new_instance
      end

      def on_raw_stderr_flush(*)
        typedb.nil_type.new_instance
      end

      def on_raw_file_open(node, _)
        node.arguments.fetch(0).type.new_instance
      end

      def on_raw_file_read(*)
        typedb.integer_type.new_instance
      end

      def on_raw_file_seek(*)
        typedb.integer_type.new_instance
      end

      def on_raw_file_size(*)
        typedb.integer_type.new_instance
      end

      def on_raw_file_write(*)
        typedb.integer_type.new_instance
      end

      def on_raw_file_remove(*)
        typedb.nil_type.new_instance
      end

      def on_raw_file_copy(*)
        typedb.integer_type.new_instance
      end

      def on_raw_file_type(*)
        typedb.integer_type.new_instance
      end

      def on_raw_file_time(*)
        typedb.integer_type.new_instance
      end

      def on_raw_directory_create(*)
        typedb.nil_type.new_instance
      end

      def on_raw_directory_remove(*)
        typedb.nil_type.new_instance
      end

      def on_raw_directory_list(*)
        typedb.new_array_of_type(typedb.string_type.new_instance)
      end

      def on_raw_drop(*)
        typedb.nil_type.new_instance
      end

      def on_raw_set_blocking(*)
        typedb.boolean_type.new_instance
      end

      def on_raw_panic(*)
        TypeSystem::Void.new
      end

      def on_raw_exit(*)
        TypeSystem::Void.new
      end

      def on_raw_platform(*)
        typedb.string_type.new_instance
      end

      def on_raw_hasher_new(node, _)
        node.arguments.fetch(0).type.new_instance
      end

      def on_raw_hasher_write(node, _)
        node.arguments.fetch(1).type
      end

      def on_raw_hasher_to_hash(*)
        typedb.integer_type.new_instance
      end

      def on_raw_hasher_reset(node, _)
        node.arguments.fetch(0).type
      end

      def on_raw_stacktrace(*)
        tuple = typedb.new_array_of_type(TypeSystem::Dynamic.new)

        typedb.new_array_of_type(tuple)
      end

      def on_raw_block_metadata(*)
        TypeSystem::Dynamic.new
      end

      def on_raw_string_format_debug(*)
        typedb.string_type.new_instance
      end

      def on_raw_string_concat_multiple(*)
        typedb.string_type.new_instance
      end

      def on_raw_byte_array_from_array(*)
        TypeSystem::Dynamic.new
      end

      def on_raw_byte_array_set(*)
        typedb.integer_type.new_instance
      end

      def on_raw_byte_array_at(*)
        typedb.integer_type.new_instance
      end

      def on_raw_byte_array_remove(*)
        typedb.integer_type.new_instance
      end

      def on_raw_byte_array_length(*)
        typedb.integer_type.new_instance
      end

      def on_raw_byte_array_clear(*)
        TypeSystem::Void.new
      end

      def on_raw_byte_array_equals(*)
        typedb.boolean_type.new_instance
      end

      def on_raw_byte_array_to_string(*)
        typedb.string_type.new_instance
      end

      def on_raw_get_boolean_prototype(*)
        typedb.boolean_type.new_instance
      end

      def on_raw_get_byte_array_prototype(*)
        typedb.byte_array_type.new_instance
      end

      def on_raw_set_object_name(*)
        typedb.string_type.new_instance
      end

      def on_raw_current_file_path(*)
        typedb.string_type.new_instance
      end

      def on_raw_env_get(*)
        TypeSystem::Optional.new(typedb.string_type.new_instance)
      end

      def on_raw_env_set(*)
        typedb.string_type.new_instance
      end

      def on_raw_env_remove(*)
        typedb.nil_type.new_instance
      end

      def on_raw_env_variables(*)
        typedb.new_array_of_type(typedb.string_type.new_instance)
      end

      def on_raw_env_home_directory(*)
        TypeSystem::Optional.new(typedb.string_type.new_instance)
      end

      def on_raw_env_temp_directory(*)
        typedb.string_type.new_instance
      end

      def on_raw_env_get_working_directory(*)
        typedb.string_type.new_instance
      end

      def on_raw_env_set_working_directory(*)
        typedb.string_type.new_instance
      end

      def on_raw_env_arguments(*)
        typedb.new_array_of_type(typedb.string_type.new_instance)
      end

      def on_raw_process_set_panic_handler(*)
        typedb.block_type.new_instance
      end

      def on_raw_process_add_defer_to_caller(*)
        TypeSystem::Block.closure(typedb.block_type)
      end

      def on_raw_set_default_panic_handler(*)
        TypeSystem::Block.lambda(typedb.block_type)
      end

      def on_raw_process_pin_thread(*)
        typedb.boolean_type.new_instance
      end

      def on_raw_process_unpin_thread(*)
        typedb.nil_type.new_instance
      end

      def on_raw_process_identifier(*)
        typedb.string_type.new_instance
      end

      def on_raw_library_open(node, _)
        node.arguments.fetch(0).type.new_instance
      end

      def on_raw_function_attach(node, _)
        node.arguments.fetch(0).type.new_instance
      end

      def on_raw_function_call(*)
        TypeSystem::Dynamic.new
      end

      def on_raw_pointer_attach(node, _)
        node.arguments.fetch(0).type.new_instance
      end

      def on_raw_pointer_read(*)
        TypeSystem::Dynamic.new
      end

      def on_raw_pointer_write(*)
        TypeSystem::Dynamic.new
      end

      def on_raw_pointer_from_address(node, _)
        node.arguments.fetch(0).type.new_instance
      end

      def on_raw_pointer_address(*)
        typedb.integer_type.new_instance
      end

      def on_raw_foreign_type_size(*)
        typedb.integer_type.new_instance
      end

      def on_raw_foreign_type_alignment(*)
        typedb.integer_type.new_instance
      end

      def on_raw_string_to_integer(*)
        typedb.integer_type.new_instance
      end

      def on_raw_string_to_float(*)
        typedb.float_type.new_instance
      end

      def on_raw_float_to_bits(*)
        typedb.integer_type.new_instance
      end

      def on_raw_socket_create(node, _)
        node.arguments.fetch(0).type.new_instance
      end

      def on_raw_socket_write(*)
        typedb.integer_type.new_instance
      end

      def on_raw_socket_read(*)
        typedb.integer_type.new_instance
      end

      def on_raw_socket_accept(node, _)
        node.arguments.fetch(0).type.new_instance
      end

      def on_raw_socket_receive_from(*)
        typedb.new_array_of_type(TypeSystem::Dynamic.new)
      end

      def on_raw_socket_send_to(*)
        typedb.integer_type.new_instance
      end

      def on_raw_socket_address(*)
        typedb.new_array_of_type(TypeSystem::Dynamic.new)
      end

      def on_raw_socket_get_option(*)
        TypeSystem::Dynamic.new
      end

      def on_raw_socket_set_option(*)
        TypeSystem::Dynamic.new
      end

      def on_raw_socket_bind(*)
        typedb.nil_type.new_instance
      end

      def on_raw_socket_connect(*)
        typedb.nil_type.new_instance
      end

      def on_raw_socket_shutdown(*)
        typedb.nil_type.new_instance
      end

      def on_raw_socket_listen(*)
        typedb.integer_type.new_instance
      end

      def on_raw_random_number(*)
        TypeSystem::Dynamic.new
      end

      def on_raw_random_range(*)
        TypeSystem::Dynamic.new
      end

      def on_raw_random_bytes(*)
        typedb.byte_array_type.new_instance
      end

      def define_block_signature(node, scope, expected_block = nil)
        define_type_parameters(node, scope)
        define_argument_types(node, scope, expected_block)
        define_throw_type(node, scope)
        define_return_type(node, scope)

        scope.define_receiver_type
        scope.block_type.define_call_method
      end

      def define_method_bounds(node, scope)
        stype = scope.self_type

        node.method_bounds.each do |bound|
          name = bound.name

          if (param = stype.lookup_type_parameter(name))
            required_traits =
              param.required_traits +
              define_types(bound.required_traits, scope)

            scope
              .block_type
              .method_bounds
              .define(name, required_traits)
          else
            diagnostics
              .undefined_type_parameter_error(stype, name, bound.location)
          end
        end
      end

      def define_type_parameters(node, scope)
        node.type_parameters.each do |param_node|
          requirements = required_traits_for_type_parameter(param_node, scope)

          scope.block_type.define_type_parameter(param_node.name, requirements)
        end
      end

      # Returns an Array containing the traits required by a type parameter.
      def required_traits_for_type_parameter(node, scope)
        requirements = []

        node.required_traits.each do |req_node|
          type = define_type(req_node, scope)

          if type&.trait?
            requirements << type
          elsif type
            diagnostics
              .invalid_type_parameter_requirement(type, req_node.location)
          else
            diagnostics.undefined_constant_error(name, req_node.location)
          end
        end

        requirements
      end

      def define_argument_types(node, scope, expected_block = nil)
        if expected_block
          define_argument_types_with_expected_block(node, scope, expected_block)
        else
          define_argument_types_without_expected_block(node, scope)
        end
      end

      def define_argument_types_without_expected_block(node, scope)
        define_types(node.arguments, scope)
      end

      def define_argument_types_with_expected_block(node, scope, expected_block)
        expected_args = expected_block.arguments

        node.arguments.zip(expected_args) do |arg_node, exp_arg|
          expected_type = exp_arg
            .type
            .resolve_type_parameters(scope.self_type, expected_block)

          define_type(arg_node, scope, expected_type)
        end
      end

      def define_throw_type(node, scope)
        return unless node.throws

        scope.block_type.throw_type = define_type_instance(node.throws, scope)
      end

      def define_return_type(node, scope)
        return unless node.returns

        scope.block_type.infer_return_type = false

        node.returns.late_binding = true

        scope.block_type.return_type =
          define_type_instance(node.returns, scope)
      end

      # Returns the type of an argument's default value, if any.
      def type_for_argument_value(arg_node, scope)
        define_type_instance(arg_node.default, scope) if arg_node.default
      end

      # Returns the type for an explicitly defined argument type, if any.
      def defined_type_for_argument(arg_node, scope)
        define_type_instance(arg_node.value_type, scope) if arg_node.value_type
      end

      # Determines which type to use for an argument.
      #
      # Given the explicitly defined type (if any) and the type of the default
      # value (if any) this method will determine which of the two should be
      # used. If neither are given the Dynamic type is used.
      #
      # rubocop: disable Metrics/CyclomaticComplexity
      # rubocop: disable Metrics/PerceivedComplexity
      def determine_argument_type(
        node,
        defined_type,
        value_type,
        block_type,
        default_type = nil
      )
        type =
          if defined_type && value_type
            unless value_type.type_compatible?(defined_type, @state)
              diagnostics
                .type_error(defined_type, value_type, node.default.location)
            end

            defined_type
          elsif defined_type
            defined_type
          elsif value_type
            value_type
          else
            default_type || TypeSystem::Dynamic.new
          end

        type.remap_using_method_bounds(block_type)
      end
      # rubocop: enable Metrics/PerceivedComplexity
      # rubocop: enable Metrics/CyclomaticComplexity

      def wrap_optional_type(node, type)
        if node.optional?
          TypeSystem::Optional.wrap(type)
        else
          type
        end
      end
    end
    # rubocop: enable Metrics/ClassLength
  end
end
