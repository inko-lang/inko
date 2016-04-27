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
          # TODO: instance variables
        end
      end

      def identifier(val_idx)
        name_idx = @code.locals.add(variable_name)

        @code.ins_set_local([name_idx, val_idx], line, column)

        name_idx
      end

      def constant(val_idx)
        name_idx = @code.strings.add(variable_name)
        self_idx = @code.next_register

        @code
          .ins_get_self([self_idx], line, column)
          .ins_set_literal_const([self_idx, val_idx, name_idx], line, column)

        name_idx
      end

      def variable_name
        variable_ast.children[1]
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
