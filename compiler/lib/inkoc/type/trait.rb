# frozen_string_literal: true

module Inkoc
  module Type
    class Trait
      include Inspect
      include ObjectOperations
      include GenericTypeOperations
      include TypeCompatibility
      include Predicates

      attr_reader :name, :attributes, :required_methods, :type_parameters,
                  :required_traits, :type_parameter_instances

      attr_accessor :prototype

      def initialize(
        name: Config::TRAIT_CONST,
        prototype: nil,
        generated: false,
        type_parameter_instances: {}
      )
        @name = name
        @prototype = prototype
        @attributes = SymbolTable.new
        @required_methods = SymbolTable.new
        @required_traits = Set.new
        @type_parameters = {}
        @type_parameter_instances = type_parameter_instances
        @generated = generated
      end

      def trait?
        true
      end

      def generated_trait?
        @generated
      end

      def new_instance(param_instances = {})
        self.class.new(
          name: name,
          prototype: self,
          type_parameter_instances: param_instances,
          generated: generated_trait?
        )
      end

      def define_required_method(block_type)
        required_methods.define(block_type.name, block_type)
      end

      def lookup_method(name)
        required_methods[name]
      end

      def type_compatible?(other)
        return true if self == other

        other.is_a?(self.class) &&
          required_traits == other.required_traits &&
          required_methods == other.required_methods
      end

      def type_name
        if generated_trait?
          type_name_for_generated_trait
        else
          super
        end
      end

      def empty?
        required_methods.empty? && required_traits.empty?
      end

      def empty_generated_trait?
        generated_trait? && empty?
      end

      def type_name_for_generated_trait
        return name unless required_traits.any?

        required_traits.map(&:type_name).join(' + ')
      end
    end
  end
end
