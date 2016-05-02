module Aeon
  module Compilation
    class Constant
      def initialize(compiler, ast, code)
        @compiler = compiler
        @ast = ast
        @code = code
      end

      def compile
        register = @code.next_register
        name_idx = @code.strings.add(name)

        receiver_reg = receiver_ast ? explicit_receiver : implicit_receiver

        @code.get_literal_const([register, receiver_reg, name_idx], line, column)

        register
      end

      def explicit_receiver
        @compiler.process(receiver_ast, @code)
      end

      def implicit_receiver
        index = @code.next_register

        @code.get_self([index], line, column)

        index
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
