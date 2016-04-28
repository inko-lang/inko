module Aeon
  module Compilation
    class Integer
      def initialize(ast, code)
        @ast = ast
        @code = code
      end

      def compile
        idx    = @code.integers.add(value)
        target = @code.next_register

        @code.set_integer([target, idx], line, column)

        target
      end

      def value
        @node.children[0]
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
