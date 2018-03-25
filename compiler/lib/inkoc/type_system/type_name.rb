# frozen_string_literal: true

module Inkoc
  module TypeSystem
    # Module providing a default implementation of the "type_name" method.
    module TypeName
      def name
        raise NotImplementedError
      end

      # Returns the name of the type, including any type parameters and their
      # instances.
      def type_name
        if type_parameters.any?
          "#{name}!(#{formatted_type_parameter_names})"
        else
          name
        end
      end

      def formatted_type_parameter_names
        params = type_parameters.map do |param|
          (lookup_type_parameter_instance(param) || param).type_name
        end

        params.join(', ')
      end
    end
  end
end
