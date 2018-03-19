# frozen_string_literal: true

module Inkoc
  module Type
    module TypeCompatibility
      def implements_trait?(trait)
        trait = trait.type if trait.optional?

        if trait.type_parameter?
          trait.required_traits.all? { |t| implements_trait?(t) }
        else
          source = self

          while source
            return true if source.implemented_traits.include?(trait)

            source = source.prototype
          end

          false
        end
      end

      def implements_all_traits?(traits)
        traits.all? { |trait| implements_trait?(trait) }
      end

      def compatible_with_constraint?(other)
        other.required_methods.all? do |_, required|
          implements_method?(required)
        end
      end

      def basic_type_compatibility?(other)
        return true if identical_or_dynamic?(other)
        return false if other.void?
        return implements_trait?(other) if check_trait_implementation?(other)
        return type_compatible?(other.type) if other.optional?
        return compatible_with_constraint?(other) if other.constraint?

        nil
      end

      def identical_or_dynamic?(other)
        self == other || other.dynamic?
      end

      def check_trait_implementation?(other)
        other.trait? || other.type_parameter?
      end

      # Returns true if the current and the given type are compatible.
      def type_compatible?(other)
        basic_compat = basic_type_compatibility?(other)

        if basic_compat.nil?
          # Generic types that are initialized set their prototype to the base
          # type, so in this case we also need to compare with the prototype of
          # the object we're comparing with.
          prototype_chain_compatible?(other)
        else
          basic_compat
        end
      end

      def strict_type_compatible?(other)
        return false if other.dynamic?

        type_compatible?(other)
      end

      def prototype_chain_compatible?(other)
        source = prototype

        while source
          return true if source.type_compatible?(other)

          source = source.prototype
        end

        false
      end
    end
  end
end
