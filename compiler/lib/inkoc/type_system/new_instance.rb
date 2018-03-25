# frozen_string_literal: true

module Inkoc
  module TypeSystem
    module NewInstance
      def base_type=(type)
        @base_type = type
      end

      def base_type
        @base_type || self
      end

      def new_instance_for_reference(instances = [])
        if instances.any?
          new_instance(instances)
        else
          self
        end
      end

      def new_instance(instances = [])
        dup.tap do |copy|
          unless type_parameters.empty?
            copy.type_parameter_instances = TypeParameterInstances.new
            copy.initialize_type_parameters_in_order(instances)
          end

          # The base type will point to the type the instance was spawned off
          # the first time "new_instance" was called. This makes it possible to
          # quickly check if a trait was implemented by using the base type,
          # instead of having to traverse over all implemented traits.
          copy.base_type = base_type
          copy.prototype = base_type
        end
      end

      def type_instance?
        base_type != self
      end

      def type_instance_of?(other)
        base_type == other
      end
    end
  end
end
