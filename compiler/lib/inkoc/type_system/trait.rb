# frozen_string_literal: true

module Inkoc
  module TypeSystem
    class Trait
      include Equality
      include Type
      include TypeWithPrototype
      include TypeWithAttributes
      include GenericType
      include GenericTypeWithInstances
      include TypeName
      include NewInstance
      include WithoutEmptyTypeParameters

      attr_reader :name, :attributes, :type_parameters, :required_traits,
                  :required_methods, :unique_id

      attr_accessor :prototype, :type_parameter_instances

      def initialize(name: Config::TRAIT_CONST, prototype: nil, unique_id: nil)
        @name = name
        @prototype = prototype
        @attributes = SymbolTable.new
        @type_parameters = TypeParameterTable.new
        @type_parameter_instances = TypeParameterInstances.new
        @required_traits = {}
        @required_methods = SymbolTable.new

        # A trait's unique ID is used by objects to quickly check if they
        # implement a certain trait, without having to rely on the (fully
        # qualified) name of the trait.
        @unique_id = unique_id
      end

      def trait?
        true
      end

      def generic_object?
        type_parameters.any?
      end

      # Returns the method for the given name.
      #
      # name - The name of a method.
      def lookup_method(name)
        lookup_attribute(name).or_else { required_methods[name] }
      end

      # Returns `true` if `self` is type compatible with `other`.
      #
      # other - The type to compare with.
      # rubocop: disable Metrics/CyclomaticComplexity
      # rubocop: disable Metrics/PerceivedComplexity
      def type_compatible?(other, state)
        other = other.type if other.optional?

        return true if other.dynamic? || self == other
        return true if other.trait? && implements_trait?(other, state)
        return compatible_with_type_parameter?(other) if other.type_parameter?

        if other.generic_object?
          return true if compatible_with_generic_type?(other, state)
        end

        prototype_chain_compatible?(other)
      end
      # rubocop: enable Metrics/CyclomaticComplexity
      # rubocop: enable Metrics/PerceivedComplexity

      def compatible_with_type_parameter?(param)
        param.required_traits.empty? || param.required_traits.include?(self)
      end

      def empty?
        required_traits.empty? && required_methods.empty?
      end

      def define_required_method(type)
        required_methods.define(type.name, type)
      end

      def implements_trait?(trait, state)
        required_traits[trait.unique_id]&.type_compatible?(trait, state)
      end

      def add_required_trait(trait)
        required_traits[trait.unique_id] = trait
      end

      def required_trait_types
        required_traits.values
      end
    end
  end
end
