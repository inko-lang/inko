module Aeon
  module Compilation
    class Closure < Method
      def initialize(compiler, ast, code)
        @compiler = compiler
        @ast = ast
        @code = code
      end

      def compile
        code_reg = @code.next_register
        closure_reg = @code.next_register
        closure = compile_body

        core_mod_idx = @code.strings.add('core')
        closure_mod_idx = @code.strings.add('closure')
        closure_name_idx = @code.strings.add('Closure')
        new_idx = @code.strings.add('new')

        core_mod_reg = @code.next_register
        closure_mod_reg = @code.next_register
        closure_class_reg = @code.next_register
        self_reg = @code.next_register

        add_implicit_return(closure)

        code_idx = @code.code_objects.add(closure)

        @code.instruct(line, column) do |ins|
          # Look up core::closure::Closure
          ins.get_self          self_reg
          ins.get_literal_const core_mod_reg, self_reg, core_mod_idx
          ins.get_literal_const closure_mod_reg, core_mod_reg, closure_mod_idx
          ins.get_literal_const closure_class_reg, closure_mod_reg, closure_name_idx

          # Closure.new(code)
          ins.set_compiled_code code_reg, code_idx
          ins.send_literal      closure_reg, closure_class_reg, new_idx, 0, code_reg
        end

        closure_reg
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
