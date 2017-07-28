# frozen_string_literal: true

module Inkoc
  module AST
    class TypeCast
      include Inspect

      attr_reader :expression, :cast_to, :location

      # expressions - The expression to cast.
      # cast_to - The type to cast the expression to.
      # location - The SourceLocation of the type-cast.
      def initialize(expression, cast_to, location)
        @expression = expression
        @cast_to = cast_to
        @location = location
      end

      def tir_process_node_method
        :on_type_cast
      end
    end
  end
end
