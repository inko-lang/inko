# frozen_string_literal: true

module Inkoc
  module Type
    class Object
      include Inspect
      include Predicates
      include ObjectOperations
      include GenericTypeOperations
      include TypeCompatibility

      attr_reader :attributes, :implemented_traits, :type_parameters, :singleton
      attr_accessor :name, :prototype

      def initialize(
        name: Config::OBJECT_CONST,
        prototype: nil,
        attributes: SymbolTable.new,
        implemented_traits: Set.new,
        type_parameters: TypeParameterTable.new,
        singleton: false
      )
        @name = name
        @prototype = prototype
        @attributes = attributes
        @implemented_traits = implemented_traits
        @type_parameters = type_parameters
        @singleton = singleton
      end

      def resolve_type(self_type, *)
        return self if type_parameters.empty?

        new_shallow_instance.tap do |object|
          object.type_parameters.initialize_self_types(self_type)
        end
      end

      def new_shallow_instance(params = type_parameters)
        return self if singleton

        new_params = TypeParameterTable.new(type_parameters)
        new_params.initialize_in_order(params)

        self.class.new(
          name: name,
          prototype: prototype,
          implemented_traits: implemented_traits,
          attributes: attributes,
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

      def ==(other)
        other.is_a?(self.class) &&
          name == other.name &&
          prototype == other.prototype &&
          attributes == other.attributes &&
          implemented_traits == other.implemented_traits
      end
    end
  end
end
