module Aeon
  module Compilation
    class InstanceVariable
      def initialize(ast, code)
        @ast = ast
        @code = code
      end

      def compile
        name_idx = @code.strings.add(name)
        self_idx = @code.next_register
        register = @code.next_register

        @code.instruct(line, column) do |ins|
          ins.get_self         self_idx
          ins.get_literal_attr register, self_idx, name_idx
        end

        register
      end

      def name
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
