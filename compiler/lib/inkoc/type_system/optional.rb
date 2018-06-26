# frozen_string_literal: true

module Inkoc
  module TypeSystem
    # An optional type `?T` is a type that can be either T or Nil.
    class Optional
      include Type
      include TypeWithPrototype
      include TypeWithAttributes
      include GenericType
      include GenericTypeWithInstances

      extend Forwardable

      attr_reader :type

      def_delegator :type, :prototype
      def_delegator :type, :attributes
      def_delegator :type, :type_parameters
      def_delegator :type, :type_parameter_instances
      def_delegator :type, :type_parameter_instances=
      def_delegator :type, :type_instance?
      def_delegator :type, :type_instance_of?
      def_delegator :type, :generic_type?
      def_delegator :type, :lookup_method
      def_delegator :type, :lookup_attribute
      def_delegator :type, :resolved_return_type
      def_delegator :type, :initialize_type_parameter?
      def_delegator :type, :lookup_type_parameter_instance
      def_delegator :type, :dynamic?

      # Wraps a type in an Optional if necessary.
      def self.wrap(type)
        if type.optional?
          type
        else
          new(type)
        end
      end

      def initialize(type)
        @type = type
      end

      def optional?
        true
      end

      # Returns a new instance of the underlying type, wrapping it in an
      # Optional.
      def new_instance(param_instances = [])
        self.class.wrap(type.new_instance(param_instances))
      end

      def type_name
        "?#{type.type_name}"
      end

      def type_compatible?(other, state)
        if other.optional? || other.type_parameter? || other.dynamic?
          type.type_compatible?(other, state)
        else
          false
        end
      end

      def dereference?
        true
      end

      def dereferenced_type
        type
      end

      def resolve_type_parameter_with_self(self_type, method_type)
        self.class.wrap(
          type.resolve_type_parameter_with_self(self_type, method_type)
        )
      end
    end
  end
end
