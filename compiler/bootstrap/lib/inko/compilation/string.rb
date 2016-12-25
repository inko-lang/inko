module Inko
  module Compilation
    class String
      def initialize(compiler, ast, code, double_quote = false)
        @compiler = compiler
        @ast = ast
        @code = code
        @double_quote = double_quote
      end

      def compile
        string = create_string

        @code.strings.add(string)

        idx = @code.strings.get(string)
        target = @code.next_register

        @code.instruction(:set_string, [target, idx], line, column)

        target
      end

      def create_string
        string = raw_string

        # TODO: is this the best way of supporting escape sequences?
        if @double_quote
          string.gsub!(/\\r|\\n|\\t/, '\n' => "\n", '\r' => "\r", '\t' => "\t")
        end

        string
      end

      def raw_string
        @ast.children[0].dup
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
