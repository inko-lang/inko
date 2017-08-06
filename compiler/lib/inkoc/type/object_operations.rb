# frozen_string_literal: true

module Inkoc
  module Type
    module ObjectOperations
      def block?
        false
      end

      def regular_object?
        false
      end

      def trait?
        false
      end

      def define_attribute(*args)
        attributes.define(*args)
      end

      def lookup_attribute(name)
        attributes[name]
      end

      def lookup_type(name)
        lookup_attribute(name)
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

      def message_return_type(name)
        type = lookup_method(name).type

        type.block? ? type.return_type : type
      end

      def responds_to_message?(name)
        lookup_method(name).type.block?
      end

      def attribute?(name)
        lookup_attribute(name).any?
      end
    end
  end
end
