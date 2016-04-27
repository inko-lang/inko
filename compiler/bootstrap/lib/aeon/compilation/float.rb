module Aeon
  module Compilation
    class Float
      def initialize(ast, code)
        @ast = ast
        @code = code
      end

      def compile
        idx    = @code.floats.add(value)
        target = @code.next_register

        @code.ins_set_float([target, idx], line, column)

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
