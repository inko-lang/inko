# frozen_string_literal: true

module Inkoc
  module TypeSystem
    # Module defining various (required) methods for regular types.
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

      def void?
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

      def dereferenced_type
        self
      end

      def new_instance(_param_instances = [])
        self
      end

      def guard_unknown_message?(name)
        optional? || dynamic? || lookup_method(name).nil?
      end

      def type_compatible?(_other, _state)
        false
      end

      def cast_to?(cast_to, state)
        dynamic? || cast_to.dynamic? || cast_to.type_compatible?(self, state)
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

      def without_empty_type_parameters
        self
      end

      def initialize_type_parameter?(_param)
        false
      end
    end
  end
end
