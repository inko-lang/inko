# frozen_string_literal: true

module Inkoc
  module TypeSystem
    module TypeWithPrototype
      def prototype
        raise NotImplementedError
      end

      # Returns `true` if our prototype chain contains the given object.
      #
      # other - The object to compare with.
      # state - An instance of `Inkoc::State`.
      def prototype_chain_compatible?(other)
        current = prototype

        while current
          return true if current == other

          current = current.prototype
        end

        false
      end
    end
  end
end
