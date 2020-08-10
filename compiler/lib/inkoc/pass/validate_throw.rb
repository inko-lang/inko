# frozen_string_literal: true

module Inkoc
  module Pass
    class ValidateThrow
      include VisitorMethods

      def initialize(compiler, mod)
        @module = mod
        @state = compiler.state
        @try_nesting = 0
        @block_nesting = []
      end

      def diagnostics
        @state.diagnostics
      end

      def run(ast)
        process_node(ast, @module.body.type)

        [ast]
      end

      def inside_block(block)
        @block_nesting << block

        yield

        @block_nesting.pop
      end

      def every_nested_block
        @block_nesting.reverse_each do |block|
          yield block
        end
      end

      def on_block(node, *)
        inside_block(node.block_type) do
          error_for_missing_throw_in_block(node, node.block_type)
        end
      end
      alias on_lambda on_block

      def on_body(node, block_type)
        process_nodes(node.expressions, block_type)
      end

      def on_define_variable(node, block_type)
        process_node(node.value, block_type)
      end
      alias on_define_variable_with_explicit_type on_define_variable

      def on_define_argument(node, block_type)
        process_node(node.default, block_type) if node.default
      end

      def on_keyword_argument(node, block_type)
        process_node(node.value, block_type)
      end

      def on_method(node, *)
        inside_block(node.block_type) do
          error_for_missing_throw_in_block(node, node.block_type)
        end
      end

      def on_node_with_body(node, *)
        process_node(node.body, node.block_type)
      end

      alias on_object on_node_with_body
      alias on_trait on_node_with_body
      alias on_trait_implementation on_node_with_body
      alias on_reopen_object on_node_with_body

      def on_raw_instruction(node, block_type)
        process_nodes(node.arguments, block_type)
      end

      def on_reassign_variable(node, block_type)
        process_node(node.value, block_type)
      end

      def on_return(node, block_type)
        process_node(node.value, block_type) if node.value
      end

      def on_send(node, block_type)
        error_for_missing_try(node)

        process_node(node.receiver, block_type) if node.receiver
        process_nodes(node.arguments, block_type)
      end

      def on_identifier(node, *)
        error_for_missing_try(node)
      end

      def on_throw(node, block_type)
        process_node(node.value, block_type)

        thrown = node.value.type

        every_nested_block do |block|
          break unless track_throw_type(thrown, block, node.location)
        end

        return if in_try?

        error_for_undefined_throw(thrown, block_type, node.location)
      end

      def on_try(node, block_type)
        @try_nesting += 1

        loc = node.location

        process_node(node.expression, block_type)
        process_node(node.else_body, block_type)

        unless node.explicit_block_for_else_body?
          if block_type == @module.body.type
            diagnostics.throw_at_top_level_error(node.throw_type, loc)
          else
            error_for_undefined_throw(node.throw_type, block_type, loc)
          end

          every_nested_block do |block|
            track_throw_type(node.throw_type, block, node.location)
          end
        end

        @try_nesting -= 1
      end

      def track_throw_type(thrown, block_type, location)
        expected = block_type.throw_type

        block_type.thrown_types << thrown if thrown

        if thrown && expected && !thrown.type_compatible?(expected, @state)
          diagnostics.type_error(expected, thrown, location)
          false
        else
          true
        end
      end

      def on_type_cast(node, block_type)
        process_node(node.expression, block_type)
      end

      def on_dereference(node, block_type)
        process_node(node.expression, block_type)
      end

      def error_for_missing_throw_in_block(node, block_type)
        process_nodes(node.arguments, block_type)
        process_node(node.body, block_type)

        expected = block_type.throw_type

        return if block_type.thrown_types.any? || !expected

        diagnostics.missing_throw_error(expected, node.location)
      end

      def error_for_missing_try(node)
        return unless (throw_type = node.block_type&.throw_type)
        return if throw_type.optional?

        diagnostics.missing_try_error(throw_type, node.location) unless in_try?
      end

      def error_for_undefined_throw(throw_type, block_type, location)
        return if block_type.throw_type
        return unless throw_type

        diagnostics.throw_without_throw_defined_error(throw_type, location)
      end

      def in_try?
        @try_nesting.positive?
      end

      def inspect
        '#<Pass::ValidateThrow>'
      end
    end
  end
end
