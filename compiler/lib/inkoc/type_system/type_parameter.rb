# frozen_string_literal: true

module Inkoc
  module TypeSystem
    # A type parameter used for generic types.
    class TypeParameter
      include Type
      include NewInstance
      include TypeWithAttributes

      attr_reader :name, :required_traits

      # name - The name of the type parameter.
      # required_traits - The traits required by the type parameter.
      def initialize(name:, required_traits: [])
        @name = name
        @required_traits = required_traits.to_set
      end

      def lookup_type_parameter_instance(_)
        nil
      end

      def attributes
        SymbolTable.new
      end

      def empty?
        required_traits.empty?
      end

      def lookup_method(name)
        required_traits.each do |trait|
          if (symbol = trait.lookup_method(name)) && symbol.any?
            return symbol
          end
        end

        NullSymbol.new(name)
      end
      alias lookup_attribute lookup_method

      def new_instance(*)
        self
      end

      def type_parameter?
        true
      end

      def type_name
        if required_traits.any?
          required_traits.map(&:type_name).join(' + ')
        else
          name
        end
      end

      def type_compatible?(other, state)
        return true if other.dynamic? || self == other

        if other.optional?
          type_compatible?(other.type, state)
        elsif other.type_parameter?
          compatible_with_type_parameter?(other)
        elsif other.trait?
          compatible_with_trait?(other)
        else
          compatible_with_object?(other)
        end
      end

      def compatible_with_type_parameter?(other)
        other.required_traits.all? { |t| required_traits.include?(t) }
      end

      def compatible_with_trait?(other)
        check = other.base_type ? other.base_type : other

        required_traits.include?(check)
      end

      def compatible_with_object?(other)
        return false if required_traits.empty?

        required_traits.all? do |trait|
          trait.prototype_chain_compatible?(other.base_type)
        end
      end

      # Initialises self in the method or self type.
      #
      # This method assumes that self and the given type are type compatible.
      def initialize_as(type, method_type, self_type)
        if method_type.initialize_type_parameter?(self)
          method_type.initialize_type_parameter(self, type)
        elsif self_type.initialize_type_parameter?(self)
          self_type.initialize_type_parameter(self, type)
        end
      end

      def remap_using_method_bounds(block_type)
        block_type.method_bounds[name] || self
      end

      def resolve_type_parameter_with_self(self_type, method_type)
        method_type.lookup_type_parameter_instance(self) ||
          self_type.lookup_type_parameter_instance(self) ||
          self
      end

      def resolve_type_parameters(self_type, method_type)
        resolve_type_parameter_with_self(self_type, method_type)
      end
    end
  end
end
