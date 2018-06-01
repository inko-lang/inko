# frozen_string_literal: true

module Inkoc
  module Pass
    # Pass that replaces keyword arguments with position arguments when passed
    # in order.
    #
    # Consider this method:
    #
    #     def register(name: String, address: String) { }
    #
    # When called like this:
    #
    #     register(name: 'Elmo', address: 'Sesame Street')
    #
    # This pass will turn the call into this:
    #
    #     register('Elmo', 'Sesame Street')
    class OptimizeKeywordArguments
      include VisitorMethods

      def initialize(mod, state)
        @module = mod
        @state = state
      end

      def run(node)
        process_nodes(node.expressions)

        [node]
      end

      def on_body(node)
        process_nodes(node.expressions)
      end

      def on_block(node)
        process_nodes(node.body.expressions)
      end
      alias on_lambda on_block

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
      alias on_define_variable on_node_with_value
      alias on_define_variable_with_explicit_type on_node_with_value
      alias on_reassign_variable on_node_with_value

      def on_type_cast(node)
        process_node(node.expression)
      end

      def on_send(node)
        process_node(node.receiver) if node.receiver

        node.arguments.map!.with_index do |arg, index|
          if arg.keyword_argument?
            if node.block_type
              on_keyword_argument(arg, index, node.block_type)
            else
              process_node(arg.value)
              arg
            end
          else
            process_node(arg)
            arg
          end
        end
      end

      def on_keyword_argument(node, position, block_type)
        symbol = block_type.arguments[node.name]

        # We add +1 to the position since "self" is the first argument but isn't
        # included explicitly in the argument list.
        if symbol.index == position + 1
          process_node(node.value)
          node.value
        else
          node
        end
      end

      def on_dereference(node)
        process_node(node.expression)
      end
    end
  end
end
