# frozen_string_literal: true

module Inkoc
  module Pass
    class SetupSymbolTables
      include VisitorMethods

      def initialize(mod, state)
        @module = mod
        @state = state
      end

      def run(node)
        on_module_body(node)

        [node]
      end

      def on_module_body(node)
        node.locals = @module.body.locals

        process_nodes(node.expressions, node)
      end

      def on_body(node, outer)
        node.locals = SymbolTable.new

        process_nodes(node.expressions, outer)
      end

      def on_block(node, outer)
        node.body.locals = SymbolTable.new(outer.locals)

        process_nodes(node.body.expressions, node.body)
      end

      def on_send(node, outer)
        process_nodes(node.arguments, outer)
        process_node(node.receiver, outer) if node.receiver
      end

      def on_node_with_body(node, *)
        process_node(node.body, node.body)
      end

      alias on_object on_node_with_body
      alias on_trait on_node_with_body
      alias on_trait_implementation on_node_with_body
      alias on_reopen_object on_node_with_body
      alias on_method on_node_with_body

      def on_try(node, outer)
        process_node(node.expression, outer)
        process_node(node.else_body, outer) if node.else_body
      end

      def on_node_with_value(node, outer)
        process_node(node.value, outer) if node.value
      end

      alias on_throw on_node_with_value
      alias on_return on_node_with_value
      alias on_define_variable on_node_with_value
      alias on_reassign_variable on_node_with_value
      alias on_keyword_argument on_node_with_value

      def on_define_argument(node, outer)
        process_node(node.default, outer) if node.default
      end

      def on_type_cast(node, outer)
        process_node(node.expression, node, outer)
      end
    end
  end
end
