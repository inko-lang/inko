# frozen_string_literal: true

module Inkoc
  module TypeSystem
    module WithoutEmptyTypeParameters
      # Removes any type parameter instances where the instances are empty type
      # parameters.
      #
      # One such case where this is necessary is when dealing with rest
      # arguments. To illustrate, consider the following method:
      #
      #     def foo!(A)(*numbers: A) -> Array!(A) {
      #       numbers
      #     }
      #
      # Now imagine we call it as follows:
      #
      #     foo
      #
      # This will result in the return type being `Array(A)` (`Array!(T -> A)`
      # to be exact). Since `A` doesn't define any required traits this makes
      # the return type rather useless, since `A` isn't a useful type. This
      # would also prevent code such as the following from compiling:
      #
      #     let x: Array!(Integer) = foo
      #
      # This is because `A` is not compatible with `Integer`.
      #
      # To work around all this this method can be used to (recursively) remove
      # all type parameters instances that use an empty type parameter. For the
      # above method this means the return type will be an uninitialised
      # `Array` (`Array!(T -> ?)` basically).
      def without_empty_type_parameters
        dup.tap do |copy|
          new_instances = TypeParameterInstances.new

          type_parameter_instances.mapping.each do |param, instance|
            next if instance.type_parameter? && instance.empty?

            new_instances.define(param, instance.without_empty_type_parameters)
          end

          copy.type_parameter_instances = new_instances
        end
      end
    end
  end
end
