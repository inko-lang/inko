module Inko
  module Compilation
    class SendInstruction
      def initialize(compiler, ast, code)
        @compiler = compiler
        @ast = ast
        @code = code
      end

      def compile
        args = process_arguments

        @code.instruction(name, args, line, column)

        args[0]
      end

      def name
        @ast.children[1][2..-1].to_sym
      end

      def arguments
        @ast.children[2..-1]
      end

      def process_arguments
        arguments.map do |node|
          if node.type == :integer
            node.children[0]

          # TODO: unescape double quoted strings
          elsif node.type == :sstring or node.type == :dstring
            @code.strings.add(node.children[0])

          elsif node.type == :ident and node.children[1] == '_'
            @code.next_register
          else
            @compiler.process(node, @code)
          end
        end
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
