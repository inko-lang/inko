# frozen_string_literal: true

module Inkoc
  module TypeSystem
    module TypeWithAttributes
      def attributes
        raise NotImplementedError
      end

      # Looks up an attribute by its name.
      #
      # name - The name of the attribute.
      def lookup_attribute(name)
        source = self

        while source
          attr = source.attributes[name]

          return attr if attr.any?

          source = source.prototype
        end

        NullSymbol.singleton
      end

      alias lookup_method lookup_attribute

      # Looks up a type by its name.
      #
      # name - The name of the type to look up.
      def lookup_type(name)
        symbol = lookup_attribute(name)

        symbol.type if symbol.any?
      end

      # Defines a new attribute.
      #
      # name - The name of the attribute.
      # value - The value to set the attribute to.
      # mutable - Whether the attribute is mutable or not.
      def define_attribute(name, value, mutable = false)
        attributes.define(name, value, mutable)
      end

      # Reassigns an existing attribute.
      def reassign_attribute(name, type)
        attributes[name].type = type
      end

      # Returns true if the type responds to the given message.
      def responds_to_message?(name)
        lookup_method(name).any?
      end

      # Returns the initialised return type of a method.
      def message_return_type(name, self_type)
        lookup_method(name).type.resolved_return_type(self_type)
      end

      # Returns true if "self" implements "method".
      def implements_method?(method, state)
        # We're using "lookup_attribute" here because "lookup_method" may be
        # redefined to use implemented or required traits, and we want to check
        # if an object _directly_ defines a required method.
        if (symbol = lookup_attribute(method.name)) && symbol.any?
          symbol.type.type_compatible?(method, state)
        else
          false
        end
      end
    end
  end
end
