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
        return true if self == other || other.dynamic?

        return implements_trait?(other) if other.trait?
        return type_compatible?(other.type) if other.optional?

        # We can pass anything to a type parameter without constraints, which is
        # rare but possible (e.g. a method that simply returns its given
        # argument).
        if other.type_parameter?
          type_compatible_with_type_parameter?(other)
        else
          prototype_chain_compatible?(other)
        end
      end

      def type_compatible_with_type_parameter?(param)
        param.required_traits.all? { |t| implements_trait?(t) } &&
          param.required_methods.all? { |m| method_implemented?(m) }
      end

      def strict_type_compatible?(other)
        return false if other.dynamic?

        type_compatible?(other)
      end
    end
  end
end
