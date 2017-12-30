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

      def type_scope_for_else(self_type)
        scope = TypeScope.new(self_type, else_block_type, else_body.locals)

        scope.define_self_local
        scope
      end

      def define_else_argument_type
        return unless (arg_name = else_argument_name)

        type = throw_type || Type::Dynamic.new

        else_block_type.define_required_argument(arg_name, type)
        else_body.locals.define(arg_name, type)
      end

      def throw_type
        btype =
          if expression.throw?
            try_block_type
          else
            expression.block_type
          end

        if btype&.physical_type?
          # For method calls (e.g. "try foo") we want the thrown type to resolve
          # to the type thrown my the "fo" method.
          btype.throws
        else
          btype
        end
      end
    end
  end
end
