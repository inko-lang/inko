module Aeon
  module Compilation
    class Method
      def initialize(compiler, ast, code)
        @compiler = compiler
        @ast = ast
        @code = code
      end

      def compile
        add_name_literal

        body = compile_body

        add_implicit_return(body)

        add_method(body)
      end

      def add_name_literal
        @code.strings.add(name)
      end

      def compile_body
        body = CompiledCode.new(name, @code.file, line, required_argument_count,
                                visibility)

        add_arguments(body)

        @compiler.process(body_ast, body)

        body
      end

      def required_argument_count
        arguments_ast.children.count do |arg|
          arg.children[2].nil?
        end
      end

      def add_arguments(code)
        arguments_ast.children.each do |arg|
          code.locals.add(arg.children[0])
        end
      end

      def add_implicit_return(code)
        ins = code.instructions.last

        unless ins.name == :return
          arg = ins.arguments[0]

          unless arg.is_a?(Fixnum)
            raise TypeError, "Can not add implicit return as #{ins.inspect} doesn't set a register"
          end

          code.ins_return([arg], ins.line, ins.column)
        end
      end

      def determine_receiver
        if receiver_ast
          @compiler.process(receiver_ast, @code)
        else
          implicit_receiver
        end
      end

      def implicit_receiver
        rec_idx = @code.next_register

        case @code.type
        when :class
          self_idx  = @code.next_register
          attr_name = @code.strings.add(Class::PROTO_ATTR)

          @code
            .ins_get_self([self_idx], line, column)
            .ins_get_literal_attr([rec_idx, self_idx, attr_name], line, column)
        # TODO: compiling methods in enums/traits
        when :enum
        when :trait
        # Method defined at the top-level
        else
          @code.ins_get_self([rec_idx], line, column)
        end

        rec_idx
      end

      def add_method(method_code)
        rec_idx = determine_receiver
        code_idx = @code.code_objects.add(method_code)
        name_idx = @code.strings.get(name)

        @code.ins_def_literal_method([rec_idx, name_idx, code_idx], line, column)

        code_idx
      end

      def receiver_ast
        @ast.children[0]
      end

      def name
        @ast.children[1]
      end

      def visibility
        @ast.children[2]
      end

      def arguments_ast
        @ast.children[4]
      end

      def body_ast
        @ast.children[6]
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
