# frozen_string_literal: true

module Inkoc
  module Pass
    class ConfigureModule
      include VisitorMethods

      def initialize(state)
        @state = state
      end

      def run(ast, mod)
        process_node(ast, mod)

        [ast, mod]
      end

      def on_body(node, mod)
        process_nodes(node.expressions, mod)
      end

      def on_compiler_option(node, mod)
        key = node.key

        if mod.config.valid_key?(key)
          mod.config[key] = node.value
        else
          diagnostics.invalid_compiler_option(key, node.location)
        end
      end

      def diagnostics
        @state.diagnostics
      end
    end
  end
end
