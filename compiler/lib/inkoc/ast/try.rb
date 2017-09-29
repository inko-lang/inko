# frozen_string_literal: true

module Inkoc
  module AST
    class Try
      include TypeOperations
      include Predicates
      include Inspect

      attr_reader :expression, :else_argument, :else_body, :location

      # expr - The expression that may throw an error.
      # else_arg - The argument to store the error in, if any.
      # else_body - The body of the "else" statement, if any.
      # location - The SourceLocation of the "try" statement.
      def initialize(expr, else_arg, else_body, location)
        @expression = expr
        @else_argument = else_arg
        @else_body = else_body
        @location = location
      end

      def visitor_method
        :on_try
      end
    end
  end
end
