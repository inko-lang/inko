# frozen_string_literal: true

module Inkoc
  module TypeSystem
    module GenericType
      def type_parameters
        raise NotImplementedError
      end

      def generic_type?
        true
      end

      def lookup_type(name)
        lookup_type_parameter(name) || super
      end

      def lookup_type_parameter(name)
        type_parameters[name]
      end

      def defines_type_parameter?(param)
        type_parameters.defines?(param)
      end

      def define_type_parameter(name, required_traits = [])
        type_parameters.define(name, required_traits)
      end
    end
  end
end
