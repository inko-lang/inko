module Aeon
  module Compilation
    class Array
      def initialize(compiler, ast, code)
        @compiler = compiler
        @ast = ast
        @code = code
      end

      def compile
        register = @code.next_register
        array_args = [register]

        values_ast.each do |ast|
          array_args << @compiler.process(ast, @code)
        end

        @code.set_array(array_args, line, column)

        register
      end

      def values_ast
        @ast.children
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
