# frozen_string_literal: true

module Inkoc
  module Type
    module ObjectOperations
      def define_attribute(*args)
        attributes.define(*args)
      end

      def lookup_attribute(name)
        source = self

        while source
          attr = attributes[name]

          return attr if attr.any?

          source = source.prototype
        end

        NullSymbol.new(name)
      end

      def type_of_attribute(name)
        symbol = lookup_attribute(name)

        symbol.any? ? symbol.type : nil
      end

      def lookup_type(name)
        lookup_attribute(name).type
      end

      def lookup_method(name, fallback: true)
        source = self

        while source
          method = source.lookup_attribute(name)

          return method unless method.nil?

          source = source.prototype
        end

        # If we didn't find anything we'll return the last looked up value,
        # which will be a NullSymbol.
        if fallback
          lookup_method(Config::UNKNOWN_MESSAGE_MESSAGE, fallback: false)
        else
          method
        end
      end

      def return_type
        self
      end

      def resolve_type(*)
        self
      end

      def message_return_type(name)
        lookup_method(name).type.return_type.resolve_type(self)
      end

      def responds_to_message?(name)
        symbol = lookup_method(name)
          .or_else { lookup_method(Config::UNKNOWN_MESSAGE_MESSAGE) }

        symbol.type.block?
      end

      def attribute?(name)
        lookup_attribute(name).any?
      end

      def if_physical_or_else
        self
      end

      def implements_method?(method_type)
        symbol = lookup_method(method_type.name)

        symbol.type.implementation_of?(method_type)
      end

      def lookup_method_from_traits(name)
        implemented_traits.each do |trait|
          if (method = trait.lookup_default_method(name)) && method.any?
            return method
          end
        end

        NullSymbol.new(name)
      end
    end
  end
end
