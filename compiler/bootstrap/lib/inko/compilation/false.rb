module Inko
  module Compilation
    class False
      def initialize(ast, code)
        @ast = ast
        @code = code
      end

      def compile
        reg = @code.next_register

        @code.get_false([reg], line, column)

        reg
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
