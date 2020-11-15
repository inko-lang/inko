# frozen_string_literal: true

module Inkoc
  module Pass
    class DesugarMethod
      include VisitorMethods

      def initialize(compiler, mod)
        @module = mod
        @state = compiler.state
      end

      def run(ast)
        process_node(ast)

        [ast]
      end

      def on_body(node)
        process_nodes(node.expressions)
      end

      def on_object(node)
        on_body(node.body)
      end

      def on_trait(node)
        on_body(node.body)
      end

      def on_trait_implementation(node)
        on_body(node.body)
      end

      def on_reopen_object(node)
        on_body(node.body)
      end

      def on_required_method(node)
        return if node.returns

        node.returns = AST::TypeName.new(
          AST::Constant.new(Config::NIL_CONST, node.location),
          [],
          node.location
        )
      end

      def on_method(node)
        return if node.returns

        last_expr = node.body.expressions.last
        ret_loc = last_expr&.location || node.body.location

        node.returns = AST::TypeName.new(
          AST::Constant.new(Config::NIL_CONST, node.location),
          [],
          node.location
        )

        return if last_expr&.return?

        # These two "if" statements are just hacks to prevent desugaring from
        # messing up tail-recursive methods. We'll need a better solution in the
        # self-hosting compiler.
        if last_expr&.send? &&
            last_expr.name == node.name &&
            (last_expr.receiver.nil? || last_expr.receiver.self?)
          return
        end

        if last_expr&.identifier? && last_expr.name == node.name
          return
        end

        # Insert a `return` at the end, unless the last expression already is a
        # `return` of some sort.
        node.body.expressions.push(AST::Return.new(nil, false, ret_loc))
      end
    end
  end
end
