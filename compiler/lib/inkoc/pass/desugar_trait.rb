# frozen_string_literal: true

module Inkoc
  module Pass
    class DesugarTrait
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

      def on_trait(node)
        defines_new = node.body.expressions.any? do |expr|
          expr.method? && expr.name == Config::NEW_MESSAGE
        end

        # If a trait defines a custom "new" method we don't want to overwrite
        # it.
        return if defines_new

        node.body.expressions.push(default_new(node.location))
        process_node(node.body)
      end

      def on_object(node)
        process_node(node.body)
      end

      # Generates a default "new" method.
      def default_new(loc)
        body = AST::Body.new([AST::Self.new(loc)], loc)

        new_return_type = AST::TypeName
          .new(AST::Constant.new(Config::SELF_TYPE, nil, loc), [], loc)

        AST::Method.new(
          Config::NEW_MESSAGE,
          [],
          [],
          new_return_type,
          nil,
          false,
          [],
          body,
          loc
        )
      end
    end
  end
end
