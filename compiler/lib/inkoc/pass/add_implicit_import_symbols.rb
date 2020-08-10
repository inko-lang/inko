# frozen_string_literal: true

module Inkoc
  module Pass
    class AddImplicitImportSymbols
      def initialize(compiler, mod)
        @module = mod
        @state = compiler.state
      end

      def run(ast)
        @module.imports.each { |import| on_import(import) }

        [ast]
      end

      def on_import(node)
        return unless node.symbols.empty?

        mod = node.steps.last

        node.symbols << AST::ImportSymbol
          .new(AST::Self.new(mod.location), mod, mod.location)
      end
    end
  end
end
