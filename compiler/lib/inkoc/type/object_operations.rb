# frozen_string_literal: true

module Inkoc
  module Type
    module ObjectOperations
      def define_attribute(*args)
        attributes.define(*args)
      end

      def lookup_attribute(name)
        attributes[name]
      end

      def lookup_method(name)
        source = self

        while source
          method = source.lookup_attribute(name)

          return method unless method.nil?

          source = source.prototype
        end

        # If we didn't find anything we'll return the last looked up value,
        # which will be a NullSymbol.
        method
      end
    end
  end
end
