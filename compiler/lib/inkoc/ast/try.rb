# frozen_string_literal: true

module Inkoc
  module AST
    class Try
      include TypeOperations
      include Predicates
      include Inspect

      attr_reader :expression, :else_argument, :else_body, :location
      attr_accessor :try_block_type, :else_block_type

      # expr - The expression that may throw an error.
      # else_body - The body of the "else" statement.
      # else_arg - The argument to store the error in, if any.
      # location - The SourceLocation of the "try" statement.
      def initialize(expr, else_body, else_arg, location)
        @expression = expr
        @else_argument = else_arg
        @else_body = else_body
        @location = location
        @try_block_type = nil
        @else_block_type = nil
      end

      def visitor_method
        :on_try
      end

      def explicit_block_for_else_body?
        else_argument || else_body.multiple_expressions?
      end

      def empty_else?
        else_body.empty?
      end

      def else_argument_name
        else_argument&.name
      end

      def throw_type
        if expression.throw?
          expression.type
        elsif expression.send?
          expression.block_type&.throw_type
        elsif expression.identifier?
          # The identifier might be a local variable, in which case "block_type"
          # is not set.
          expression.block_type&.throw_type
        end
      end
    end
  end
end
