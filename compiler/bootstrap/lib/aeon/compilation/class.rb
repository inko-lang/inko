module Aeon
  module Compilation
    class Class
      PROTOTYPE = '__iproto'

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

        @code.get_self([register], line, column)

        register
      end

      def explicit_parent
        parent_reg = @code.next_register

        if parent_ast.children[0]
          psource = @compiler.process(parent_ast.children[0], @code)
        else
          psource = @code.next_register

          @code.get_self([psource], line, column)
        end

        parent_name = @code.strings.add(parent_ast.children[1])

        @code.instruct(line, column) do |ins|
          # Get the parent class object
          ins.get_literal_const parent_reg, psource, parent_name
        end

        parent_reg
      end

      def implicit_parent
        core_mod_idx = @code.strings.add('core')
        object_mod_idx = @code.strings.add('object')
        object_name_idx = @code.strings.add('Object')

        core_mod_reg = @code.next_register
        obj_mod_reg = @code.next_register
        parent_reg = @code.next_register
        self_idx = @code.next_register

        @code.instruct(line, column) do |ins|
          # Look up core::object::Object in the current scope.
          ins.get_self          self_idx
          ins.get_literal_const core_mod_reg, self_idx, core_mod_idx
          ins.get_literal_const obj_mod_reg, core_mod_reg, object_mod_idx
          ins.get_literal_const parent_reg, obj_mod_reg, object_name_idx
        end

        parent_reg
      end

      def create_or_reopen(name_source, parent_reg)
        class_name_idx = @code.strings.add(class_name)
        core_mod_name_idx = @code.strings.add('core')
        class_mod_name_idx = @code.strings.add('class')
        class_class_name_idx = @code.strings.add('Class')
        new_name_idx = @code.strings.add('new')

        core_mod_reg = @code.next_register
        class_mod_reg = @code.next_register
        class_class_reg = @code.next_register

        exists_reg = @code.next_register
        class_reg = @code.next_register
        top_reg = @code.next_register
        send_reg = @code.next_register
        true_reg = @code.next_register

        jump_to = @code.label

        @code.instruct(line, column) do |ins|
          # Checks if the constant already exists or not.
          ins.literal_const_exists exists_reg, name_source, class_name_idx

          # If the constant already exists we'll jump to the last instruction in
          # this block.
          ins.goto_if_true jump_to, exists_reg

          # Look up core::class::Class
          ins.get_toplevel      top_reg
          ins.get_literal_const core_mod_reg, top_reg, core_mod_name_idx
          ins.get_literal_const class_mod_reg, core_mod_reg, class_mod_name_idx
          ins.get_literal_const class_class_reg, class_mod_reg, class_class_name_idx

          # core::class::Class.new(parent_class, true)
          ins.get_true     true_reg
          ins.send_literal send_reg, class_class_reg, new_name_idx, 0, 0,
            parent_reg, true_reg

          # Define the class as a constant.
          ins.set_literal_const name_source, class_name_idx, send_reg

          # Get the class object, which at this point is guaranteed to exist.
          ins.mark_label        jump_to
          ins.get_literal_const class_reg, name_source, class_name_idx
        end

        class_reg
      end

      def create_and_run_body(class_idx)
        body_code = create_body

        body_idx = @code.code_objects.add(body_code)
        body_ret_idx = @code.next_register

        @code.run_literal_code([body_ret_idx, body_idx, class_idx], line, column)
      end

      def create_body
        body_code = CompiledCode
          .new(class_name, @code.file, line, visibility: :public, type: :class)

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
