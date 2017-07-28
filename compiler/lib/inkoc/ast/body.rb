# frozen_string_literal: true

module Inkoc
  module AST
    class Body
      include Inspect

      attr_reader :expressions, :location

      # expr - The expressions of this body.
      # location - The SourceLocation of this node.
      def initialize(expr, location)
        @expressions = expr
        @location = location
      end

      def tir_process_node_method
        :on_body
      end
    end
  end
end
