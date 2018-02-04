# frozen_string_literal: true

module Inkoc
  module Pass
    class ValidateThrow
      include VisitorMethods

      def initialize(mod, state)
        @module = mod
        @state = state
        @try_nesting = 0
        @throws = []
      end

      def diagnostics
        @state.diagnostics
      end

      def run(ast)
        process_node(ast, @module.body.type)

        [ast]
      end

      def on_block(node, *)
        error_for_missing_throw_in_block(node)
      end
      alias on_lambda on_block

      def on_body(node, block_type)
        process_nodes(node.expressions, block_type)
      end

      def on_define_variable(node, block_type)
        process_node(node.value, block_type)
      end

      def on_define_argument(node, block_type)
        process_node(node.default, block_type) if node.default
      end

      def on_keyword_argument(node, block_type)
        process_node(node.value, block_type)
      end

      def on_method(node, *)
        error_for_missing_throw_in_block(node) unless node.required?
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

        location = node.location
        exp_throw = block_type.throws

        if exp_throw && !node.value.type.type_compatible?(exp_throw)
          diagnostics.type_error(exp_throw, node.value.type, location)
        end

        increment_throws

        return if in_try?

        error_for_undefined_throw(node.value.type, block_type, location)
      end

      def on_try(node, block_type)
        @try_nesting += 1

        loc = node.location

        process_node(node.expression, block_type)
        process_node(node.else_body, block_type)

        if node.else_body.empty?
          if block_type == @module.body.type
            diagnostics.throw_at_top_level_error(node.throw_type, loc)
          else
            error_for_undefined_throw(node.throw_type, block_type, loc)
          end
        end

        @try_nesting -= 1
      end

      def on_type_cast(node, block_type)
        process_node(node.expression, block_type)
      end

      def error_for_missing_throw_in_block(node)
        throws = throws? do
          process_nodes(node.arguments, node.block_type)
          process_node(node.body, node.block_type)
        end

        ttype = node.block_type.throws

        return if throws || !ttype

        diagnostics.missing_throw_error(ttype, node.location)
      end

      def error_for_missing_try(node)
        return unless (throw_type = node.block_type&.throws)
        return if throw_type.optional?

        diagnostics.missing_try_error(throw_type, node.location) unless in_try?
      end

      def error_for_undefined_throw(throw_type, block_type, location)
        return if block_type.throws

        diagnostics.throw_without_throw_defined_error(throw_type, location)
      end

      def in_try?
        @try_nesting.positive?
      end

      def throws?
        @throws << 0
        yield
        @throws.pop.positive?
      end

      def increment_throws
        @throws[-1] += 1 if track_throw?
      end

      def track_throw?
        @throws.any?
      end

      def inspect
        '#<Pass::ValidateThrow>'
      end
    end
  end
end
