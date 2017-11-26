# frozen_string_literal: true

module Inkoc
  module Type
    class Optional
      include Inspect
      include Predicates
      include ObjectOperations
      include GenericTypeOperations

      attr_reader :type

      def initialize(type)
        @type = type
      end

      def optional?
        true
      end

      def block?
        type.block?
      end

      def regular_object?
        type.regular_object?
      end

      def trait?
        type.trait?
      end

      def type_parameters
        type.type_parameters
      end

      def attributes
        type.attributes
      end

      def type_name
        type.type_name
      end
    end
  end
end
