# frozen_string_literal: true

module Inkoc
  module Pass
    class DefineTypes
      include TypeLookup
      include DefineTypeParameters
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
        nodes.each { |node| define_type(node, self_type, locals) }
      end

      def run(ast)
        locals = ast.locals

        on_module_body(ast, locals)

        # Method bodies are processed last since they may depend on types
        # defined after the method itself is defined.
        @method_bodies.each do |method|
          define_type(method.ast, method.self_type, method.locals)
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

        define_type(ast, @module.type, locals)
      end

      def define_module_type
        top = typedb.top_level
        modules = top.lookup_attribute(Config::MODULES_ATTRIBUTE).type
        proto = top.lookup_attribute(Config::MODULE_TYPE).type
        type = Type::Object.new(@module.name.to_s, proto)

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
          .or_else { @module.lookup_attribute(name) }

        diagnostics.undefined_constant_error(name, node.location) if symbol.nil?

        symbol.type
      end

      def on_identifier(node, self_type, locals)
        name = node.name
        symbol = locals[name].or_else { self_type.lookup_method(name) }

        if symbol.nil?
          diagnostics.undefined_method_error(self_type, name, node.location)
        end

        symbol.type.return_type
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
        if node.raw_instruction?
          return on_raw_instruction(node, self_type, locals)
        end

        name = node.name
        rec_type =
          if node.receiver
            define_type(node.receiver, self_type, locals)
          else
            self_type
          end

        symbol = rec_type.lookup_method(node.name)

        unless symbol.type.block?
          diagnostics.undefined_method_error(rec_type, name, node.location)

          return symbol.type
        end

        arg_types = node.arguments.map do |arg|
          define_type(arg, self_type, locals)
        end

        symbol.type.initialized_return_type(arg_types)
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

        Type::Object.new(nil, proto)
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
        proto = typedb.top_level.lookup_attribute(Config::OBJECT_CONST).type
        name = node.name
        type = Type::Object.new(name, proto)

        type
          .define_attribute(Config::NAME_INSTANCE_ATTRIBUTE, typedb.string_type)

        define_type_parameters(node.type_parameters, type)
        store_type(type, self_type)
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
        proto = typedb.top_level.lookup_attribute(Config::TRAIT_CONST).type
        name = node.name
        type = Type::Trait.new(name, proto)

        define_type_parameters(node.type_parameters, type)

        node.required_traits.each do |trait|
          trait_type = type_for_constant(trait, [self_type, @module])

          type.required_traits << trait_type if trait_type.trait?
        end

        store_type(type, self_type)
        define_type(node.body, type, node.body.locals)

        type
      end

      def on_trait_implementation(node, self_type, *)
        trait = type_for_constant(node.trait_name, [self_type, @module])
        object = type_for_constant(node.object_name, [self_type, @module])

        define_block_type_for_object(node, object)
        define_type(node.body, object, node.body.locals)

        # TODO: check if all required methods are implemented
        # TODO: check if methods of inherited traits are implemented
        trait.required_traits.each do |req_trait|
          next if object.trait_implemented?(req_trait)

          diagnostics
            .uninplemented_trait_error(trait, object, req_trait, node.location)
        end

        trait.required_methods.each do |method|
          next if object.method_implemented?(method)

          diagnostics
            .unimplemented_method_error(method.type, object, node.location)
        end

        # TODO: only do this when everything is OK
        object.implemented_traits << trait

        trait
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
          store_type(type, self_type)
        end

        @method_bodies << DeferredMethod
          .new(node.body, self_type, node.body.locals)

        type
      end

      def on_block(node, self_type, *)
        type = Type::Block.new(Config::BLOCK_NAME, typedb.block_prototype)

        block_signature(node, type, self_type, node.body.locals)
        define_type(node.body, self_type, node.body.locals)

        rtype = node.body.type

        type.returns = rtype if type.returns.dynamic?

        unless rtype.type_compatible?(type.return_type)
          diagnostics.return_type_error(type.returns, rtype, node.location)
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
        store_type(value_type, self_type, node.variable.name)
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
        existing_type = self_type.lookup_attribute(node.variable.name).type

        return if value_type.type_compatible?(existing_type)

        diagnostics.type_error(existing_type, value_type, node.value.location)
      end

      def on_reassign_local(node, _, value_type, locals)
        name = node.variable.name
        local = locals[name]
        existing_type = local.type

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
        block_type.returns =
          if node.returns
            wrap_optional_type(
              node.returns,
              type_for_constant(node.returns, [block_type, self_type, @module])
            )
          else
            Type::Dynamic.new
          end
      end

      def define_throw_type(node, block_type, self_type)
        return unless node.throws

        block_type.throws = wrap_optional_type(
          node.returns,
          type_for_constant(node.throws, [block_type, self_type, @module])
        )
      end

      def type_for_argument_value(arg, self_type, locals)
        define_type(arg.default, self_type, locals) if arg.default
      end

      def defined_type_for_argument(arg, block_type, self_type)
        return unless arg.type

        wrap_optional_type(
          arg.type,
          type_for_constant(arg.type, [block_type, self_type, @module])
        )
      end

      def argument_types_incompatible?(defined_type, value_type)
        defined_type && value_type && !defined_type.type_compatible?(value_type)
      end

      def store_type(type, self_type, name = type.name)
        self_type.define_attribute(name, type)

        @module.globals.define(name, type) if module_scope?(self_type)
      end

      def module_scope?(self_type)
        self_type == @module.type
      end

      def wrap_optional_type(node, type)
        node.optional? ? Type::Optional.new(type) : type
      end
    end
  end
end
