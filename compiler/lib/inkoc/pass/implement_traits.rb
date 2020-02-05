# frozen_string_literal: true

module Inkoc
  module Pass
    # A compiler pass that simply marks a trait as implement. Trait
    # implementations are verified in a separate pass.
    class ImplementTraits
      include VisitorMethods
      include TypePass

      def on_body(node, scope)
        node.expressions.each do |expr|
          define_type(expr, scope) if expr.trait_implementation?
        end

        nil
      end

      def on_trait_implementation(node, scope)
        object = define_type(node.object_name, scope)

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

        nil
      end
    end
  end
end
