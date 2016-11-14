module Aeon
  module Compilation
    class Let
      def initialize(compiler, ast, code)
        @compiler = compiler
        @ast = ast
        @code = code
      end

      def compile
        val_idx = @compiler.process(value_ast, @code)

        case variable_ast.type
        when :ident
          identifier(val_idx)
        when :const
          constant(val_idx)
        when :ivar
          instance_variable(val_idx)
        end

        val_idx
      end

      def identifier(val_idx)
        name_idx = @code.locals.add(variable_name)

        @code.set_local([name_idx, val_idx], line, column)
      end

      def constant(val_idx)
        name_idx = @code.strings.add(variable_name)
        self_idx = @code.next_register

        @code.instruct(line, column) do |ins|
          ins.get_self          self_idx
          ins.set_literal_const self_idx, name_idx, val_idx
        end
      end

      def instance_variable(val_idx)
        name_idx = @code.strings.add(variable_name)
        self_idx = @code.next_register

        @code.instruct(line, column) do |ins|
          ins.get_self         self_idx
          ins.set_literal_attr self_idx, name_idx, val_idx
        end
      end

      def variable_name
        idx = variable_ast.type == :ivar ? 0 : 1

        variable_ast.children[idx]
      end

      def variable_ast
        @ast.children[0]
      end

      def value_ast
        @ast.children[2]
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
