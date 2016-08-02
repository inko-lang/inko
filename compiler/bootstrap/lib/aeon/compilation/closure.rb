module Aeon
  module Compilation
    class Closure < Method
      def initialize(compiler, ast, code)
        @compiler = compiler
        @ast = ast
        @code = code
      end

      def compile
        register = @code.next_register
        closure = compile_body

        add_implicit_return(closure)

        code_idx = @code.code_objects.add(closure)

        # TODO: use Closure.new
        @code.instruct(line, column) do |ins|
          ins.set_compiled_code register, code_idx
        end

        register
      end

      def code_for_body
        code = super
        code.outer_scope = @code
        code
      end

      def name
        '<closure>'
      end

      def type
        :closure
      end

      def body_ast
        @ast.children[2]
      end

      def arguments_ast
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
