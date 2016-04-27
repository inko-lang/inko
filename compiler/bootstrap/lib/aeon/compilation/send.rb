module Aeon
  module Compilation
    class Send
      def initialize(compiler, ast, code)
        @compiler = compiler
        @ast = ast
        @code = code
      end

      def compile
        add_name_literal

        name_idx = @code.strings.get(name)

        args = process_arguments

        if receiver_ast
          rec = explicit_receiver
        else
          rec = implicit_receiver
        end

        target = @code.next_register
        vis = 1

        # TODO: properly determine visibility
        @code.ins_send_literal([target, rec, name_idx, vis, args.length, *args],
                               line, column)

        target
      end

      def explicit_receiver
        @compiler.process(receiver_ast, @code)
      end

      def implicit_receiver
        index = @code.next_register

        @code.ins_get_self([index], line, column)

        index
      end

      def receiver_ast
        @ast.children[0]
      end

      def name
        @ast.children[1]
      end

      def arguments
        @ast.children[2..-1]
      end

      def add_name_literal
        @code.strings.add(name)
      end

      def process_arguments
        arguments.map { |arg| @compiler.process(arg, @code) }
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
