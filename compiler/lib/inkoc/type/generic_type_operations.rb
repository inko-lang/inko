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
        type_params = type_parameter_names.map do |name|
          if (instance = type_parameter_instances[name])
            instance.type_name
          else
            param = lookup_type_parameter(name)

            param.required_traits.map(&:type_name).join(' + ')
          end
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
