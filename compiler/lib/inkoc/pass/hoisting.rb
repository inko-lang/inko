# frozen_string_literal: true

module Inkoc
  module Pass
    # Pass that hoists constant, type, and method definitions to the start of
    # their enclosing scope.
    class Hoisting
      include VisitorMethods

      def initialize(mod, state)
        @module = mod
        @state = state
      end

      def run(ast)
        process_node(ast)

        [ast]
      end

      def on_body(node)
        types = []
        methods = []

        node.expressions.reject! do |expr|
          hoist = expr.hoist?

          process_node(expr.body) if expr.hoist_children?

          if hoist && expr.method?
            methods << expr
          elsif hoist
            types << expr
          end

          hoist
        end

        node.prepend(methods)
        node.prepend(types)
      end
    end
  end
end
