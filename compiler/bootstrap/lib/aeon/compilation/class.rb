module Aeon
  module Compilation
    class Class
      PROTO_ATTR = '__prototype'

      def initialize(compiler, ast, code)
        @compiler = compiler
        @ast = ast
        @code = code
      end

      def compile
        register = create_or_reopen(name_source, parent_source)

        create_and_run_body(register)

        register
      end

      def name_source
        name_receiver? ? explicit_name_source : implicit_name_source
      end

      def parent_source
        parent_ast ? explicit_parent : implicit_parent
      end

      def explicit_name_source
        @compiler.process(name_ast.children[0], @code)
      end

      def implicit_name_source
        register = @code.next_register

        @code.ins_get_self([register], line, column)

        register
      end

      def explicit_parent
        parent_reg = @code.next_register
        parent_class_reg = @code.next_register
        proto_name_idx = @code.strings.add(PROTO_ATTR)

        if parent_ast.children[0]
          psource = @compiler.process(parent_ast.children[0], @code)
        else
          psource = @code.next_register

          @code.ins_get_self([psource], line, column)
        end

        parent_name = @code.strings.add(parent_ast.children[1])

        @code
          .ins_get_literal_const([parent_class_reg, psource, parent_name], line, column)
          .ins_get_literal_attr([parent_reg, parent_class_reg, proto_name_idx], line, column)

        parent_reg
      end

      def implicit_parent
        parent_reg = @code.next_register
        parent_class_reg = @code.next_register
        proto_name_idx = @code.strings.add(PROTO_ATTR)

        self_idx = @code.next_register
        parent_name = @code.strings.add('Object')

        @code
          .ins_get_self([self_idx], line, column)
          .ins_get_literal_const([parent_class_reg, self_idx, parent_name], line, column)
          .ins_get_literal_attr([parent_reg, parent_class_reg, proto_name_idx], line, column)

        parent_reg
      end

      def create_or_reopen(name_source, parent_reg)
        name_idx = @code.strings.add(class_name)
        proto_name_idx = @code.strings.add('__prototype')

        exists_reg = @code.next_register
        target_reg = @code.next_register
        proto_reg  = @code.next_register

        @code.ins_literal_const_exists([exists_reg, name_source, name_idx],
                                        line, column)

        jump_to = @code.label

        @code
          .ins_goto_if_true([jump_to, exists_reg], line, column)
          .ins_set_object([target_reg], line, column)
          .ins_set_object([proto_reg], line, column)
          .ins_set_prototype([proto_reg, parent_reg], line, column)
          .ins_set_literal_attr([target_reg, proto_reg, proto_name_idx], line, column)
          .ins_set_literal_const([name_source, target_reg, name_idx], line, column)
          .ins_get_literal_const([target_reg, name_source, name_idx], line, column)

        @code.mark_label(jump_to)

        target_reg
      end

      def create_and_run_body(class_idx)
        body_code = create_body

        body_idx = @code.code_objects.add(body_code)
        body_ret_idx = @code.next_register

        @code.ins_run_literal_code([body_ret_idx, body_idx, class_idx], line,
                                   column)
      end

      def create_body
        body_code = CompiledCode
          .new(class_name, @code.file, line, 0, :public, :class)

        @compiler.process(body_ast, body_code)

        body_code
      end

      def name_receiver?
        !!name_ast.children[0]
      end

      def name_ast
        name = @ast.children[0]

        if name.type == :type
          name = name.children[0]
        end

        name
      end

      def class_name
        name_ast.children[1]
      end

      def parent_ast
        @ast.children[1]
      end

      def body_ast
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
