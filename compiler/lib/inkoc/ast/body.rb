# frozen_string_literal: true

module Inkoc
  module AST
    class Body
      include Predicates
      include Inspect

      attr_reader :location
      attr_accessor :expressions

      # expr - The expressions of this body.
      # location - The SourceLocation of this node.
      def initialize(expr, location)
        @expressions = expr
        @location = location
      end

      def visitor_method
        :on_body
      end

      def last_expression
        @expressions.last
      end

      def prepend(nodes)
        return if nodes.empty?

        @expressions = nodes + @expressions
      end
    end
  end
end
