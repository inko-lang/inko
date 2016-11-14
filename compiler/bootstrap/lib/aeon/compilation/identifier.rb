module Aeon
  module Compilation
    class Identifier
      def initialize(compiler, ast, code)
        @compiler = compiler
        @ast = ast
        @code = code
      end

      def compile
        if @code.local_defined?(name)
          depth, local_idx = @code.resolve_local(name)
          register = @code.next_register

          if depth
            @code.get_parent_local([register, depth, local_idx], line, column)
          else
            @code.get_local([register, local_idx], line, column)
          end

          register

        # Method call or a reference to a lower-cased constant (e.g. a package)
        else
          lookup_done = @code.label
          send_message = @code.label

          const_exists_reg = @code.next_register
          register = @code.next_register
          receiver_reg = receiver
          name_idx = @code.strings.add(name)

          @code.instruct(line, column) do |ins|
            ins.literal_const_exists const_exists_reg, receiver_reg, name_idx
            ins.goto_if_false        send_message, const_exists_reg
            ins.get_literal_const    register, receiver_reg, name_idx
            ins.goto                 lookup_done
          end

          @code.mark_label(send_message)

          Compilation::Send.new(@compiler, @ast, @code).compile(register)

          @code.mark_label(lookup_done)

          register
        end
      end

      def receiver
        receiver_ast ? explicit_receiver : implicit_receiver
      end

      def explicit_receiver
        @compiler.process(receiver_ast, @code)
      end

      def implicit_receiver
        register = @code.next_register

        @code.get_self([register], line, column)

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
