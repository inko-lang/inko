# frozen_string_literal: true

module Inkoc
  module Pass
    class ConfigureModule
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
        process_nodes(node.expressions)
      end

      def on_compiler_option(node)
        key = node.key

        if @module.config.valid_key?(key)
          @module.config[key] = node.value
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
