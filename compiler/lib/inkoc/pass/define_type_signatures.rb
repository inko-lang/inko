# frozen_string_literal: true

module Inkoc
  module Pass
    # Compiler pass that defines the signatures of objects and traits.
    class DefineTypeSignatures
      include VisitorMethods
      include TypePass

      def on_body(node, scope)
        node.expressions.each do |expr|
          define_type(expr, scope) if expr.trait? || expr.object?
        end

        nil
      end

      def on_object(node, scope)
        type = typedb.new_object_type(node.name)

        define_object_name_attribute(type)
        define_named_type(node, type, scope)
      end

      def on_trait(node, scope)
        if (existing = scope.lookup_type(node.name))
          return diagnostics
            .redefine_existing_constant_error(existing, node.location)
        end

        trait_proto = @module.globals[Config::TRAIT_CONST].type

        unless trait_proto
          raise "Trait's can't be defined until std::trait::Trait is defined." \
            " This is likely a bootstrapping/compiler bug"
        end

        type = typedb.new_trait_type(node.name, trait_proto)

        define_object_name_attribute(type)
        define_required_traits(node, type, scope)
        define_named_type(node, type, scope)
      end

      def on_define_type_parameter(node, scope)
        traits = define_types(node.required_traits, scope)

        scope.self_type.define_type_parameter(node.name, traits)
      end

      def define_object_name_attribute(type)
        type.define_attribute(
          Config::OBJECT_NAME_INSTANCE_ATTRIBUTE,
          typedb.string_type.new_instance
        )
      end

      def define_named_type(node, new_type, scope)
        body_type = TypeSystem::Block.closure(typedb.block_type)
        body_scope = TypeScope
          .new(new_type, body_type, @module, locals: node.body.locals)

        body_scope.define_receiver_type

        node.block_type = body_type

        define_types(node.type_parameters, body_scope)
        store_type(new_type, scope, node.location)

        new_type
      end
    end
  end
end
