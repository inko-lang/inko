# frozen_string_literal: true

module Inkoc
  module Type
    module TypeCompatibility
      def implements_trait?(trait)
        if trait.empty_generated_trait?
          true
        elsif trait.generated_trait?
          trait.required_traits.all? { |t| implements_trait?(t) }
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

      def basic_type_compatibility?(other)
        return true if self == other || other.dynamic?
        return false if other.void?
        return implements_trait?(other) if other.trait?
        return type_compatible?(other.type) if other.optional?

        false
      end

      # Returns true if the current and the given type are compatible.
      def type_compatible?(other)
        return true if basic_type_compatibility?(other)

        prototype_chain_compatible?(other)
      end

      def strict_type_compatible?(other)
        return false if other.dynamic?

        type_compatible?(other)
      end
    end
  end
end
