module Aeon
  module Compilation
    class Return
      def initialize(compiler, ast, code)
        @compiler = compiler
        @ast = ast
        @code = code
      end

      def compile
        index = @compiler.process(expression, @code)

        @code.ins_return([index], line, column)

        index
      end

      def expression
        @ast.children[0]
      end

      def line
        @ast.line
      end

      def column
        @ast.column
      end
    end
  end
end
