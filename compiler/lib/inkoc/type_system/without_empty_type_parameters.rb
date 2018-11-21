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
      def without_empty_type_parameters(self_type, block_type)
        dup.tap do |copy|
          new_instances = TypeParameterInstances.new

          type_parameter_instances.mapping.each do |param, instance|
            next if instance.type_parameter? && instance.empty?

            if instance.type_parameter? &&
               self_type.lookup_type_parameter_instance(instance).nil? &&
               block_type.lookup_type_parameter_instance(instance).nil?
              # When mapping a type parameter to an uninitialised type
              # parameter, discard the mapping. This way, return types that
              # include unitialised type parameters can be inferred
              # appropriately. An example:
              #
              #     def map!(T: Equal)(
              #       keys: Array!(T),
              #       values: Array!(T)
              #     ) -> HashMap!(T, T) {
              #       ...
              #     }
              #
              #     map([], [])
              #
              # Without this logic, the return type would be:
              #
              #     HashMap!(K -> Equal, V -> Equal)
              #
              # This would then prevent us from doing the following, because
              # `Equal` is not compatible with `String`:
              #
              #     let mapping: HashMap!(String, String) = map([], [])
              #
              # By removing the uninitialised type parameters, we essentially
              # produce the following type in this example:
              #
              #     HashMap!(K -> ?, V -> ?)
              #
              # This then allows the compiler to infer the proper type in the
              # `let` above, instead of producing an error.
              next
            end

            new_instances.define(
              param,
              instance.without_empty_type_parameters(self_type, block_type)
            )
          end

          copy.type_parameter_instances = new_instances
        end
      end
    end
  end
end
