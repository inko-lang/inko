# frozen_string_literal: true

module Inkoc
  module AST
    class TypeCast
      include TypeOperations
      include Inspect
      include Predicates

      attr_reader :expression, :cast_to, :location

      def initialize(expression, cast_to, location)
        @expression = expression
        @cast_to = cast_to
        @location = location
      end

      def visitor_method
        :on_type_cast
      end
    end
  end
end
