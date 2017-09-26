# frozen_string_literal: true

module Inkoc
  module Type
    module TypeCompatibility
      def implements_trait?(trait)
        implemented_traits.include?(trait)
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
        return true if self == other
        return true if other.dynamic?

        return implements_trait?(other) if other.trait?
        return type_compatible?(other.type) if other.optional?

        prototype_chain_compatible?(other)
      end
    end
  end
end