# frozen_string_literal: true

module Inkoc
  module Type
    module TypeCompatibility
      def implements_trait?(trait)
        if trait.empty_generated_trait?
          true
        else
          implemented_traits.include?(trait)
        end
      end

      def prototype_chain_compatible?(other)
        proto = prototype

        while proto
          return true if proto.type_compatible?(other)

          proto = proto.prototype
        end

        false
      end

      # Returns true if the current and the given type are compatible.
      def type_compatible?(other)
        return true if self == other || other.dynamic?

        return implements_trait?(other) if other.trait?
        return type_compatible?(other.type) if other.optional?

        prototype_chain_compatible?(other)
      end

      def strict_type_compatible?(other)
        return false if other.dynamic?

        type_compatible?(other)
      end
    end
  end
end
