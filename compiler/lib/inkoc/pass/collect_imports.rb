# frozen_string_literal: true

module Inkoc
  module Pass
    class CollectImports
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
        node.expressions.reject! do |exp|
          process_node(exp)
          exp.import?
        end
      end

      def on_import(node)
        @module.imports << node
      end
    end
  end
end
