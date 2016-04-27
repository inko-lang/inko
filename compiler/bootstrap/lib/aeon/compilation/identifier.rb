module Aeon
  module Compilation
    class Identifier
      def initialize(ast, code)
        @ast = ast
        @code = code
      end

      def compile
        # TODO: determine when to use a lvar and when to use a send
        local_idx = @code.locals.add(name)
        register  = @code.next_register

        @code.ins_get_local([register, local_idx], line, column)

        register
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
