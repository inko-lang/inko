# frozen_string_literal: true

module Inkoc
  module TypeSystem
    # A regular object defined using the "object" keyword.
    class Object
      include Equality
      include Type
      include TypeWithPrototype
      include TypeWithAttributes
      include GenericType
      include GenericTypeWithInstances
      include TypeName
      include NewInstance
      include WithoutEmptyTypeParameters

      attr_reader :name, :attributes, :type_parameters, :implemented_traits

      attr_accessor :prototype, :type_parameter_instances

      # name - The name of the object as a String.
      # prototype - The prototype of the object, if any.
      def initialize(name: Config::OBJECT_CONST, prototype: nil, builtin: false)
        @name = name
        @prototype = prototype
        @attributes = SymbolTable.new
        @type_parameters = TypeParameterTable.new
        @type_parameter_instances = TypeParameterInstances.new
        @implemented_traits = {}
        @builtin = builtin
      end

      def builtin?
        @builtin
      end

      def object?
        true
      end

      def generic_object?
        type_parameters.any?
      end

      # Returns `true` if we are compatible with the given object.
      #
      # other - The object to compare with.
      # state - An instance of `Inkoc::State`.
      # rubocop: disable Metrics/CyclomaticComplexity
      # rubocop: disable Metrics/PerceivedComplexity
      def type_compatible?(other, state)
        return true if other.dynamic? || self == other
        return compatible_with_optional?(other, state) if other.optional?
        return compatible_with_trait?(other) if other.trait?

        if other.type_parameter?
          return compatible_with_type_parameter?(other, state)
        end

        if other.generic_object?
          compatible_with_generic_type?(other, state)
        else
          prototype_chain_compatible?(other)
        end
      end
      # rubocop: enable Metrics/CyclomaticComplexity
      # rubocop: enable Metrics/PerceivedComplexity

      # Returns `true` if we are compatible with the given optional type.
      #
      # other - The optional object to compare with.
      # state - An instance of `Inkoc::State`.
      def compatible_with_optional?(other, state)
        if type_compatible?(other.type, state)
          true
        else
          optional_marker_implemented?(state)
        end
      end

      # Returns `true` if we implement the marker `std::marker::Optional`.
      def optional_marker_implemented?(state)
        marker_implemented?(Config::OPTIONAL_CONST, state)
      end

      # Returns `true` if we implement the marker with the given name.
      #
      # name - The name of the marker as defined in `std::marker`
      # state - An instance of `Inkoc::State`.
      def marker_implemented?(name, state)
        if (marker = state.type_of_module_global(Config::MARKER_MODULE, name))
          implements_trait?(marker)
        else
          false
        end
      end

      # Returns `true` if we are compatible with the given trait.
      #
      # other - A trait to compare with.
      def compatible_with_trait?(other)
        implements_trait?(other.base_type || other)
      end

      # Initialises any type parameters in self as the given type.
      #
      # This method requires that both self and the given type are type
      # compatible.
      def initialize_as(type, method_type, self_type)
        return unless type.generic_type?

        type_parameters.zip(type.type_parameters) do |ours, theirs|
          to_init = lookup_type_parameter_instance(ours)
          init_as = type.lookup_type_parameter_instance(theirs)

          to_init&.initialize_as(init_as, method_type, self_type) if init_as
        end
      end

      def lookup_method(name)
        super.or_else { lookup_method_from_implemented_traits(name) }
      end

      def lookup_method_from_implemented_traits(name)
        implemented_traits.each do |_, trait|
          symbol = trait.lookup_method(name)

          return symbol if symbol.any?
        end

        NullSymbol.singleton
      end

      def implement_trait(trait)
        # This is a hack due to trait lookups being messy and leading to stack
        # overflows. In the new Inko compiler we'll have to come up with a sane
        # way of looking up default methods from a parent object.
        trait.attributes.each do |symbol|
          define_attribute(symbol.name, symbol.type) if symbol.type.method?
        end

        implemented_traits[trait.unique_id] = trait
      end

      def implements_trait?(trait, *)
        if implemented_traits.key?(trait.unique_id)
          true
        elsif prototype
          prototype.implements_trait?(trait)
        else
          false
        end
      end

      def remove_trait_implementation(trait)
        implemented_traits.delete(trait.unique_id)
      end

      def lookup_type_parameter_instance(param)
        if (instance = super)
          return instance
        end

        implemented_traits.each do |_, trait|
          instance = trait.lookup_type_parameter_instance(param)

          next unless instance

          if instance.type_parameter?
            # Sometimes a trait's parameter A points to type parameter B defined
            # in `self`. In this case we want the instance that is mapped to B,
            # not B itself.
            return lookup_type_parameter_instance(instance)
          end

          return instance
        end

        nil
      end
    end
  end
end
