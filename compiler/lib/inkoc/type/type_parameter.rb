# frozen_string_literal: true

module Inkoc
  module Type
    class TypeParameter
      include Inspect
      include Predicates

      attr_reader :name, :required_traits

      def initialize(name:, required_traits: [])
        @name = name
        @required_traits = required_traits.to_set
      end

      def new_instance(*)
        self
      end

      def type_parameter?
        true
      end

      def trait?
        true
      end

      def prototype
        nil
      end

      def lookup_method(name)
        required_traits.each do |trait|
          if (method = trait.lookup_method(name)) && method.any?
            return method
          end
        end

        NullSymbol.new(name)
      end

      def message_return_type(name)
        lookup_method(name).type.return_type
      end

      def resolve_type(self_type, type_parameters = self_type.type_parameters)
        type_parameters.instance_for(name) || self
      end

      def type_name
        return name if required_traits.empty?

        required_traits.map(&:type_name).join(' + ')
      end

      def type_compatible?(other)
        other.dynamic? || strict_type_compatible?(other)
      end

      def strict_type_compatible?(other)
        other.type_parameter? && required_traits == other.required_traits
      end
    end
  end
end
