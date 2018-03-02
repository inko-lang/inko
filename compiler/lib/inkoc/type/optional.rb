# frozen_string_literal: true

module Inkoc
  module Type
    class Optional
      include Inspect
      include Predicates
      include ObjectOperations

      extend Forwardable

      def_delegator :type, :generic_type?
      def_delegator :type, :type_parameter?
      def_delegator :type, :block?
      def_delegator :type, :regular_object?
      def_delegator :type, :trait?
      def_delegator :type, :lookup_method

      attr_reader :type

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

      def resolve_type(*args)
        self.class.wrap(type.resolve_type(*args))
      end

      def initialize_as(*args)
        self.class.wrap(type.initialize_as(*args))
      end

      def type_name
        "?#{type.type_name}"
      end

      # rubocop: disable Style/MethodMissing
      def method_missing(name, *args, &block)
        type.public_send(name, *args, &block)
      end
      # rubocop: enable Style/MethodMissing

      def respond_to_missing?(name, include_private = false)
        type.respond_to?(name, include_private)
      end

      def type_compatible?(other)
        if other.optional? || other.dynamic?
          super
        else
          false
        end
      end
    end
  end
end
