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
        body = CompiledCode.new(name, @code.file, line, argument_count,
                                required_argument_count,
                                rest_argument: rest_argument?,
                                visibility: visibility)

        add_arguments(body)

        define_default_arguments(body)

        @compiler.process(body_ast, body)

        body
      end

      def rest_argument
        arguments_ast.children.find { |arg| arg.type == :restarg }
      end

      def rest_argument?
        arguments_ast.children.any? { |arg| arg.type == :restarg }
      end

      def argument_count
        arguments_ast.children.count { |arg| arg.type == :arg }
      end

      def required_argument_count
        arguments_ast.children.count do |arg|
          arg.children[2].nil? && arg.type != :restarg
        end
      end

      def add_arguments(code)
        arguments_ast.children.each do |arg|
          name = arg.children[0]

          if code.locals.include?(name)
            raise CompileError, "The argument #{name.inspect} already exists"
          else
            code.locals.add(name)
          end
        end
      end

      def define_default_arguments(body)
        arguments_ast.children.each do |arg|
          default = arg.children[2]

          next unless default

          name = arg.children[0]
          local_idx = body.locals.get(name)
          exists_reg = body.next_register

          jump_to = body.label

          body.instruct(default.line, default.column) do |ins|
            ins.local_exists exists_reg, local_idx
            ins.goto_if_true jump_to, exists_reg
            ins.set_local    local_idx, @compiler.process(default, body)
            ins.mark_label   jump_to
          end
        end
      end

      def add_implicit_return(code)
        ins = code.instructions.last

        unless ins.name == :return
          arg = ins.arguments[0]

          unless arg.is_a?(Fixnum)
            raise TypeError, "Can not add implicit return as #{ins.inspect} doesn't set a register"
          end

          code.return([arg], ins.line, ins.column)
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
          attr_name = @code.strings.add(Class::PROTOTYPE)

          @code.instruct(line, column) do |ins|
            ins.get_self         self_idx
            ins.get_literal_attr rec_idx, self_idx, attr_name
          end
        # TODO: compiling methods in enums/traits
        when :enum
        when :trait
        # Method defined at the top-level
        else
          @code.get_self([rec_idx], line, column)
        end

        rec_idx
      end

      def add_method(method_code)
        register = @code.next_register
        rec_idx = determine_receiver
        code_idx = @code.code_objects.add(method_code)
        name_idx = @code.strings.get(name)

        @code.def_literal_method([register, rec_idx, name_idx, code_idx],
                                 line, column)

        register
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
