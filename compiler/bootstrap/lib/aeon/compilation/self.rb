module Aeon
  module Compilation
    class Self
      def initialize(ast, code)
        @ast = ast
        @code = code
      end

      def compile
        index = @code.next_register

        @code.ins_get_self([index], line, column)

        index
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
