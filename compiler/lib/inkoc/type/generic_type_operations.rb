# frozen_string_literal: true

module Inkoc
  module Type
    module GenericTypeOperations
      def define_type_parameter(name, param)
        type_parameters[name] = param
      end

      def init_type_parameter(name, type)
        type_parameter_instances[name] = type
      end

      def type_parameter_names
        source = self

        while source
          params = source.type_parameters

          return params.keys if params.any?

          source = source.prototype
        end

        []
      end

      def lookup_type_parameter_instance(name)
        type_parameter_instances[name]
      end

      def lookup_type_parameter(name)
        source = self

        while source
          symbol = source.type_parameters[name]

          return symbol if symbol

          source = source.prototype
        end

        nil
      end

      def lookup_type(name)
        symbol = lookup_attribute(name)

        return symbol.type if symbol.any?

        lookup_type_parameter_instance(name) || lookup_type_parameter(name)
      end

      def type_name
        tname =
          if name
            name
          else
            trait? ? 'Trait' : 'Object'
          end

        type_params = type_parameter_names.map do |name|
          instance = type_parameter_instances[name]

          instance ? instance.type_name : '?'
        end

        if type_params.any?
          "#{tname}!(#{type_params.join(', ')})"
        else
          tname
        end
      end
    end
  end
end
