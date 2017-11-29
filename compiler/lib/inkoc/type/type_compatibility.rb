# frozen_string_literal: true

module Inkoc
  module Type
    module TypeCompatibility
      def implements_trait?(trait)
        if trait.type_parameter?
          trait.required_traits.all? { |t| implements_trait?(t) }
        else
          implemented_traits.include?(trait)
        end
      end

      def basic_type_compatibility?(other)
        return true if self == other || other.dynamic?
        return false if other.void?
        return implements_trait?(other) if other.trait?
        return type_compatible?(other.type) if other.optional?

        nil
      end

      # Returns true if the current and the given type are compatible.
      def type_compatible?(other)
        basic_compat = basic_type_compatibility?(other)

        if basic_compat.nil?
          prototype == other
        else
          basic_compat
        end
      end

      def strict_type_compatible?(other)
        return false if other.dynamic?

        type_compatible?(other)
      end
    end
  end
end
