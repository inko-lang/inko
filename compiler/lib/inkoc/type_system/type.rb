# frozen_string_literal: true

module Inkoc
  module TypeSystem
    # Module defining various (required) methods for regular types.
    # rubocop: disable Metrics/ModuleLength
    module Type
      def type_name
        raise NotImplementedError, "#{self.class} does not implement #type_name"
      end

      def block?
        false
      end

      def method?
        false
      end

      def closure?
        false
      end

      def lambda?
        false
      end

      def object?
        false
      end

      def dynamic?
        false
      end

      def trait?
        false
      end

      def optional?
        false
      end

      def error?
        false
      end

      def never?
        false
      end

      def generic_type?
        false
      end

      def generic_object?
        false
      end

      def type_parameter?
        false
      end

      def dereference?
        false
      end

      def self_type?
        false
      end

      def dereferenced_type
        self
      end

      def new_instance(_param_instances = [])
        self
      end

      def lookup_unknown_message(state)
        mod = state.modules[Config::UNKNOWN_MESSAGE_MODULE]
        method_name = Config::UNKNOWN_MESSAGE_MESSAGE

        trait = mod&.lookup_type(Config::UNKNOWN_MESSAGE_TRAIT)

        if trait && implements_trait?(trait, state)
          lookup_method(method_name)
        else
          NullSymbol.new(method_name)
        end
      end

      def implements_trait?(*)
        false
      end

      def guard_unknown_message?(name)
        optional? || dynamic? || lookup_method(name).nil?
      end

      def type_compatible?(_other, _state)
        false
      end

      # Returns true if `self` is compatible with the given type parameter.
      def compatible_with_type_parameter?(param, state)
        param.required_traits.all? do |trait|
          type_compatible?(trait, state)
        end
      end

      def cast_to?(cast_to, state)
        return cast_to?(cast_to.type, state) if cast_to.optional?

        cast_to.type_compatible?(self, state) ||
          state.typedb.object_type.type_compatible?(self, state)
      end

      def resolve_self_type(_self_type)
        self
      end

      def downcast_to(_other)
        self
      end

      def initialize_as(_type, _method_type, _self_type)
        nil
      end

      def implementation_of?(_block, _state)
        false
      end

      def remap_using_method_bounds(_block_type)
        self
      end

      def without_empty_type_parameters(_self_type, _block_type)
        self
      end

      def initialize_type_parameter?(_param)
        false
      end

      def resolve_type_parameters(_self_type, _method_type)
        self
      end

      def resolve_type_parameter_with_self(_self_type, _method_type)
        self
      end
    end
    # rubocop: enable Metrics/ModuleLength
  end
end
