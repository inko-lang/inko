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

      def lookup_method(name)
        source = self

        while source
          method = source.lookup_attribute(name)

          return method unless method.nil?

          source = source.prototype
        end

        NullSymbol.new(name)
      end

      def return_type
        self
      end

      def resolve_type(*)
        self
      end

      def with_method_requirements(*)
        self
      end

      def message_return_type(name)
        lookup_method(name).type.return_type.resolve_type(self)
      end

      def responds_to_message?(name)
        lookup_method(name).type.block?
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

      def unknown_message_return_type
        lookup_method(Config::UNKNOWN_MESSAGE_MESSAGE).type.return_type
      end

      def guard_unknown_message?(name)
        dynamic? || optional? || lookup_method(name).nil?
      end

      def downcast_to(*)
        self
      end
    end
  end
end
