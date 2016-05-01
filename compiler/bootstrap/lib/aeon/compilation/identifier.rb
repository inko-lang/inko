module Aeon
  module Compilation
    class Identifier
      def initialize(compiler, ast, code)
        @compiler = compiler
        @ast = ast
        @code = code
      end

      def compile
        if @code.locals.include?(name)
          local_idx = @code.locals.get(name)
          register  = @code.next_register

          @code.get_local([register, local_idx], line, column)

          register
        else
          Compilation::Send.new(@compiler, @ast, @code).compile
        end
      end

      def receiver_ast
        @ast.children[0]
      end

      def name
        @ast.children[1]
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
