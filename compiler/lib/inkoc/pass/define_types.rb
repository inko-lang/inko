# frozen_string_literal: true

module Inkoc
  module Pass
    class DefineTypes
      include TypeLookup
      include DefineTypeParameters

      def initialize(state)
        @state = state
        @type_inference = TypeInference.new(state)
      end

      def diagnostics
        @state.diagnostics
      end

      def typedb
        @state.typedb
      end

      def run(ast, mod)
        process_node(ast, mod.type, mod)

        [ast, mod]
      end

      def process_node(node, type, mod)
        callback = node.visitor_method

        public_send(callback, node, type, mod) if respond_to?(callback)
      end

      def on_body(node, self_type, mod)
        node.expressions.each do |expr|
          process_node(expr, self_type, mod)
        end
      end

      def on_object(node, self_type, mod)
        proto = type_of_global(Config::OBJECT_BUILTIN, node.location, mod)
        name = node.name
        type = Type::Object.new(name, proto)

        define_type_parameters(node.type_parameters, type, mod)
        implement_traits(node.trait_implementations, type, self_type, mod)

        store_type(type, self_type, mod)

        process_node(node.body, type, mod)
      end

      def on_trait(node, self_type, mod)
        proto = type_of_global(Config::TRAIT_BUILTIN, node.location, mod)
        name = node.name
        type = Type::Trait.new(name, proto)

        define_type_parameters(node.type_parameters, type, mod)
        implement_traits(node.trait_implementations, type, self_type, mod)

        store_type(type, self_type, mod)

        process_node(node.body, type, mod)
      end

      def on_method(node, self_type, mod)
        type = Type::Block.new(typedb.block_prototype, name: node.name)

        define_type_parameters(node.type_parameters, type, mod)
        define_arguments(node.arguments, type, self_type, mod)
        define_return_type(node, type, self_type, mod)
        define_throw_type(node, type, self_type, mod)

        if node.required?
          if self_type.trait?
            self_type.define_required_method(type)
          else
            diagnostics.define_required_method_on_non_trait_error(node.location)
          end
        else
          store_type(type, self_type, mod)
        end
      end

      def on_define_variable(node, self_type, mod)
        callback = node.variable.define_variable_visitor_method

        public_send(callback, node, self_type, mod) if respond_to?(callback)
      end

      def on_define_constant(node, self_type, mod)
        name = node.variable.name
        vtype = @type_inference.infer(node.value, self_type, mod)

        store_type(vtype, self_type, mod, name)
      end

      alias on_define_attribute on_define_constant

      def define_arguments(arguments, block_type, self_type, mod)
        block_type.arguments.define(Config::SELF_LOCAL, self_type)

        arguments.each do |arg|
          val_type = type_for_argument_value(arg, self_type, mod)
          def_type = defined_type_for_argument(arg, block_type, self_type, mod)

          # If both an explicit type and default value are given we need to make
          # sure the two are compatible.
          if argument_types_incompatible?(def_type, val_type)
            diagnostics.type_error(def_type, val_type, arg.default.location)
          end

          block_type
            .arguments
            .define(arg.name, def_type || val_type || Type::Dynamic.new)

          block_type.rest_argument = true if arg.rest?
        end
      end

      def define_return_type(node, block_type, self_type, mod)
        block_type.returns =
          if node.returns
            wrap_optional_type(
              node.returns,
              type_for_constant(node.returns, [block_type, self_type, mod])
            )
          else
            Type::Dynamic.new
          end
      end

      def define_throw_type(node, block_type, self_type, mod)
        return unless node.throws

        block_type.throws = wrap_optional_type(
          node.returns,
          type_for_constant(node.throws, [block_type, self_type, mod])
        )
      end

      def type_for_argument_value(arg, self_type, mod)
        @type_inference.infer(arg.default, self_type, mod) if arg.default
      end

      def defined_type_for_argument(arg, block_type, self_type, mod)
        return unless arg.type

        wrap_optional_type(
          arg.type,
          type_for_constant(arg.type, [block_type, self_type, mod])
        )
      end

      def argument_types_incompatible?(defined_type, value_type)
        defined_type && value_type && !defined_type.type_compatible?(value_type)
      end

      def store_type(type, self_type, mod, name = type.name)
        self_type.define_attribute(name, type)

        mod.globals.define(name, type) if module_scope?(self_type, mod)
      end

      def implement_traits(traits, type, self_type, mod)
        traits.each do |trait|
          type.implemented_traits <<
            type_for_constant(trait.name, [self_type, mod])
        end
      end

      def module_scope?(self_type, mod)
        self_type == mod.type
      end

      def type_of_global(name, location, mod)
        symbol = mod.globals[name]

        diagnostics.undefined_constant_error(name, location) if symbol.nil?

        symbol.type
      end

      def wrap_optional_type(node, type)
        node.optional? ? Type::Optional.new(type) : type
      end
    end
  end
end
