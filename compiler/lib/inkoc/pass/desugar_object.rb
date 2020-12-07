# frozen_string_literal: true

module Inkoc
  module Pass
    class DesugarObject
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
        impls = []

        node.expressions.each do |expr|
          next unless expr.object?

          impls.push(add_object_implementation(expr)) if @module.import_bootstrap?
        end

        node.expressions.concat(impls)
      end

      def add_object_implementation(node)
        AST::TraitImplementation.new(
          AST::TypeName.new(
            AST::Constant.new(Config::OBJECT_CONST, node.location),
            [],
            node.location
          ),
          AST::Constant.new(node.name, node.location),
          AST::Body.new([], node.location),
          node.location
        )
      end
    end
  end
end
