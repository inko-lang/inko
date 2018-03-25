# frozen_string_literal: true

module Inkoc
  module TypeSystem
    module GenericTypeWithInstances
      def type_parameter_instances
        raise NotImplementedError
      end

      def lookup_type_parameter_instance(param)
        type_parameter_instances[param]
      end

      def initialize_type_parameter(param, value)
        type_parameter_instances.define(param, value)
      end

      def initialize_type_parameters_in_order(instances)
        type_parameters.zip(instances).each do |param, instance|
          initialize_type_parameter(param, instance) if instance
        end
      end

      def initialize_type_parameter?(param)
        defines_type_parameter?(param) && !type_parameter_instances[param]
      end

      # Returns true if our type is compatible with the given generic type.
      def compatible_with_generic_type?(other, state)
        # This ensures we're not comparing an A!(X) with a B!(X), which are two
        # completely different types. This automatically takes into account the
        # number of type parameters as these are defined on the base type (and
        # thus the same after this check).
        return false if base_type != other.base_type

        # If we share the same base type, but we are not initialised yet, then
        # we are compatible with "other". One such example would be comparing an
        # empty Array with a type `Array!(Foo)`.
        return true if type_parameter_instances.empty?

        type_parameters.all? do |param|
          ours = lookup_type_parameter_instance(param) || param
          theirs = other.lookup_type_parameter_instance(param) || param

          ours.type_compatible?(theirs, state)
        end
      end
    end
  end
end
