# frozen_string_literal: true

module Inkoc
  module Type
    class Object
      include Inspect
      include Predicates
      include ObjectOperations
      include GenericTypeOperations
      include TypeCompatibility

      attr_reader :attributes, :implemented_traits, :type_parameters,
                  :type_parameter_instances

      attr_accessor :name, :prototype

      def initialize(name = nil, prototype = nil, type_param_instances = {})
        @name = name
        @prototype = prototype
        @attributes = SymbolTable.new
        @implemented_traits = Set.new
        @type_parameters = {}
        @type_parameter_instances = type_param_instances
      end

      def new_instance(type_parameter_instances = {})
        self.class.new(name, self, type_parameter_instances)
      end

      def regular_object?
        true
      end

      def type_parameter_instances_compatible?(other)
        return false unless other.regular_object?
        return true if other.type_parameter_instances.empty?

        type_parameter_instances.all? do |name, type|
          other_type = other.lookup_type_parameter_instance(name)

          other_type ? type.type_compatible?(other_type) : false
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

      def trait_implemented?(trait)
        implemented_traits.include?(trait)
      end

      def method_implemented?(method)
        symbol = lookup_method(method.name)

        symbol.type.implementation_of?(method.type)
      end
    end
  end
end
