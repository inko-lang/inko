# frozen_string_literal: true

module Inkoc
  module Pass
    class AddDefaultForRestArguments
      include VisitorMethods

      def initialize(mod, state)
        @module = mod
        @state = state
      end

      def run(node)
        process_node(node)

        [node]
      end

      def on_body(node)
        process_nodes(node.expressions)
      end

      def on_block(node)
        process_nodes(node.body.expressions)
      end

      def on_send(node)
        process_nodes(node.arguments)
        process_node(node.receiver) if node.receiver
      end

      def on_node_with_body(node)
        process_node(node.body)
      end

      alias on_object on_node_with_body
      alias on_trait on_node_with_body
      alias on_trait_implementation on_node_with_body
      alias on_reopen_object on_node_with_body
      alias on_method on_node_with_body

      def on_try(node)
        process_node(node.expression)
        process_node(node.else_body) if node.else_body
      end

      def on_node_with_value(node)
        process_node(node.value) if node.value
      end

      alias on_throw on_node_with_value
      alias on_return on_node_with_value
      alias on_reassign_variable on_node_with_value
      alias on_define_variable on_node_with_value

      def on_define_argument(node)
        return unless node.rest?

        receiver = AST::Global.new(Config::ARRAY_CONST, node.location)
        node.default = AST::Send.new('new', receiver, [], node.location)
      end
    end
  end
end
