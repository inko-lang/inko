# frozen_string_literal: true

module Inkoc
  module Type
    module GenericTypeOperations
      def generic_type?
        type_parameters.any?
      end

      def define_type_parameter(name, required_traits = [])
        type_parameters.define(name, required_traits)
      end

      def initialize_type_parameter(name, type)
        type_parameters.initialize_parameter(name, type)
      end

      def type_parameter_names
        type_parameters.names
      end

      def lookup_type_parameter_instance(name)
        type_parameters.instance_for(name)
      end

      def lookup_type_parameter(name)
        type_parameters[name]
      end

      def lookup_type(name)
        symbol = lookup_attribute(name)

        return symbol.type if symbol.any?

        lookup_type_parameter_instance_or_parameter(name)
      end

      def lookup_type_parameter_instance_or_parameter(name)
        lookup_type_parameter_instance(name) || lookup_type_parameter(name)
      end

      def type_name
        type_params = type_parameters.map do |param|
          instance = lookup_type_parameter_instance(param.name)

          instance&.type_name || param.type_name
        end

        if type_params.any?
          "#{name}!(#{type_params.join(', ')})"
        else
          name
        end
      end
    end
  end
end
