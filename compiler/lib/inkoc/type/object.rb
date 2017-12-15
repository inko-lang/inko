# frozen_string_literal: true

module Inkoc
  module Type
    class Object
      include Inspect
      include Predicates
      include ObjectOperations
      include GenericTypeOperations
      include TypeCompatibility

      attr_reader :attributes, :implemented_traits, :type_parameters
      attr_accessor :name, :prototype

      def initialize(
        name: Config::OBJECT_CONST,
        prototype: nil,
        implemented_traits: Set.new,
        type_parameters: TypeParameterTable.new
      )
        @name = name
        @prototype = prototype
        @attributes = SymbolTable.new
        @implemented_traits = implemented_traits
        @type_parameters = type_parameters
      end

      def new_instance(params = type_parameters)
        new_params = TypeParameterTable.new(type_parameters)
        new_params.initialize_in_order(params)

        self.class.new(
          name: name,
          prototype: self,
          implemented_traits: implemented_traits.dup,
          type_parameters: new_params
        )
      end

      def regular_object?
        true
      end

      def type_parameter_instances_compatible?(other)
        return false unless other.regular_object?

        type_parameters.each_instance.all? do |name, our_type|
          their_type = other.lookup_type_parameter_instance(name)

          their_type ? our_type.type_compatible?(their_type) : true
        end
      end

      def type_compatible?(other)
        valid = super

        if other.regular_object?
          valid && type_parameter_instances_compatible?(other)
        else
          valid
        end
      end

      def lookup_method(name, *)
        super.or_else { lookup_method_from_traits(name) }
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
