# frozen_string_literal: true

module Inkoc
  module Pass
    class CodeGeneration
      include VisitorMethods

      def initialize(mod, state)
        @module = mod
        @state = state
      end

      def run
        [process_node(@module.body)]
      end

      def on_code_object(code_object)
        compiled_code = Codegen::CompiledCode
          .new(code_object.name.to_s, code_object.location)

        process_instructions(compiled_code, code_object)
        assign_compiled_code_metadata(compiled_code, code_object)

        compiled_code
      end

      def process_instructions(compiled_code, code_object)
        code_object.blocks.each do |basic_block|
          basic_block.instructions.each do |tir_ins|
            process_node(tir_ins, compiled_code, basic_block)
          end
        end
      end

      def assign_compiled_code_metadata(compiled_code, code_object)
        compiled_code.arguments = code_object.argument_names
        compiled_code.required_arguments = code_object.required_arguments_count
        compiled_code.rest_argument = code_object.rest_argument?
        compiled_code.locals = code_object.local_variables_count
        compiled_code.registers = code_object.registers_count
        compiled_code.captures = code_object.captures?

        set_catch_entries(compiled_code, code_object)
      end

      def set_catch_entries(compiled_code, code_object)
        entries = code_object.catch_table.entries

        compiled_code.catch_table = entries.map do |entry|
          start = entry.try_block.instruction_offset
          stop = entry.try_block.instruction_end
          jump_to = entry.else_block.instruction_offset

          Codegen::CatchEntry.new(start, stop, jump_to, entry.register.id)
        end
      end

      def on_get_global(tir_ins, compiled_code, *)
        register = tir_ins.register.id
        variable = tir_ins.variable.index

        compiled_code
          .instruct(:GetGlobal, [register, variable], tir_ins.location)
      end

      def on_get_local(tir_ins, compiled_code, *)
        register = tir_ins.register.id
        variable = tir_ins.variable.index

        compiled_code
          .instruct(:GetLocal, [register, variable], tir_ins.location)
      end

      def on_get_parent_local(tir_ins, compiled_code, *)
        register = tir_ins.register.id
        depth = tir_ins.depth
        variable = tir_ins.variable.index
        location = tir_ins.location

        compiled_code
          .instruct(:GetParentLocal, [register, depth, variable], location)
      end

      def on_set_parent_local(tir_ins, compiled_code, *)
        depth = tir_ins.depth
        variable = tir_ins.variable.index
        value = tir_ins.value.id
        location = tir_ins.location

        compiled_code
          .instruct(:SetParentLocal, [variable, depth, value], location)
      end

      def on_goto_next_block_if_true(tir_ins, compiled_code, basic_block)
        index = basic_block.next.instruction_offset
        register = tir_ins.register.id

        compiled_code.instruct(:GotoIfTrue, [index, register], tir_ins.location)
      end

      def on_skip_next_block(tir_ins, compiled_code, basic_block)
        index = basic_block.next.next.instruction_offset

        compiled_code.instruct(:Goto, [index], tir_ins.location)
      end

      def on_local_exists(tir_ins, compiled_code, *)
        register = tir_ins.register.id
        variable = tir_ins.variable.index

        compiled_code
          .instruct(:LocalExists, [register, variable], tir_ins.location)
      end

      def on_return(tir_ins, compiled_code, *)
        block_return = tir_ins.block_return ? 1 : 0
        register = tir_ins.register.id

        compiled_code
          .instruct(:Return, [block_return, register], tir_ins.location)
      end

      def on_run_block(tir_ins, compiled_code, *)
        register = tir_ins.register.id
        block = tir_ins.block.id
        args = tir_ins.arguments.map(&:id)
        kwargs = tir_ins.keyword_arguments.map(&:id)
        ins_args = [
          register,
          block,
          args.length,
          kwargs.length / 2,
          *args,
          *kwargs
        ]

        compiled_code.instruct(:RunBlock, ins_args, tir_ins.location)
      end

      def on_tail_call(tir_ins, compiled_code, *)
        args = tir_ins.arguments.map(&:id)
        kwargs = tir_ins.keyword_arguments.map(&:id)
        ins_args = [args.length, kwargs.length / 2, *args, *kwargs]

        compiled_code
          .instruct(:TailCall, ins_args, tir_ins.location)
      end

      def on_set_array(tir_ins, compiled_code, *)
        register = tir_ins.register.id
        values = tir_ins.values.map(&:id)

        compiled_code.instruct(:SetArray, [register, *values], tir_ins.location)
      end

      def on_set_attribute(tir_ins, compiled_code, *)
        reg = tir_ins.register.id
        rec = tir_ins.receiver.id
        name = tir_ins.name.id
        val = tir_ins.value.id

        compiled_code
          .instruct(:SetAttribute, [reg, rec, name, val], tir_ins.location)
      end

      def on_set_block(tir_ins, compiled_code, *)
        register = tir_ins.register.id
        block_code = process_node(tir_ins.code_object)
        code_index = compiled_code.code_objects.add(block_code)

        compiled_code
          .instruct(:SetBlock, [register, code_index], tir_ins.location)
      end

      def on_set_hash_map(*)
        raise NotImplementedError, '#on_set_hash_map is not yet implemented'
      end

      def on_set_literal(tir_ins, compiled_code, *)
        lit = compiled_code.literals.get_or_set(tir_ins.value)
        reg = tir_ins.register.id

        compiled_code.instruct(:SetLiteral, [reg, lit], tir_ins.location)
      end

      def on_set_object(tir_ins, compiled_code, *)
        reg = tir_ins.register.id
        perm = tir_ins.permanent.id
        args =
          if (proto = tir_ins.prototype)
            [reg, perm, proto.id]
          else
            [reg, perm]
          end

        compiled_code.instruct(:SetObject, args, tir_ins.location)
      end

      def on_set_prototype(tir_ins, compiled_code, *)
        object = tir_ins.object.id
        proto = tir_ins.prototype.id

        compiled_code.instruct(:SetPrototype, [object, proto], tir_ins.location)
      end

      def on_set_local(tir_ins, compiled_code, *)
        var = tir_ins.variable.index
        val = tir_ins.value.id

        compiled_code.instruct(:SetLocal, [var, val], tir_ins.location)
      end

      def on_set_global(tir_ins, compiled_code, *)
        reg = tir_ins.register.id
        var = tir_ins.variable.index
        val = tir_ins.value.id

        compiled_code.instruct(:SetGlobal, [reg, var, val], tir_ins.location)
      end

      def on_nullary(tir_ins, compiled_code, *)
        reg = tir_ins.register.id

        compiled_code.instruct(tir_ins.name, [reg], tir_ins.location)
      end

      def on_unary(tir_ins, compiled_code, *)
        reg = tir_ins.register.id
        operand = tir_ins.operand.id

        compiled_code.instruct(tir_ins.name, [reg, operand], tir_ins.location)
      end

      def on_binary(tir_ins, compiled_code, *)
        reg = tir_ins.register.id
        base = tir_ins.base.id
        other = tir_ins.other.id

        compiled_code
          .instruct(tir_ins.name, [reg, base, other], tir_ins.location)
      end

      def on_ternary(tir_ins, compiled_code, *)
        reg = tir_ins.register.id
        one = tir_ins.one.id
        two = tir_ins.two.id
        three = tir_ins.three.id

        compiled_code
          .instruct(tir_ins.name, [reg, one, two, three], tir_ins.location)
      end

      def on_process_suspend_current(tir_ins, compiled_code, *)
        timeout = tir_ins.timeout.id

        compiled_code
          .instruct(:ProcessSuspendCurrent, [timeout], tir_ins.location)
      end

      def on_array_set(tir_ins, compiled_code, *)
        reg = tir_ins.register.id
        array = tir_ins.array.id
        index = tir_ins.index.id
        value = tir_ins.value.id

        compiled_code
          .instruct(:ArraySet, [reg, array, index, value], tir_ins.location)
      end

      def on_copy_blocks(tir_ins, compiled_code, *)
        to = tir_ins.to.id
        from = tir_ins.from.id

        compiled_code.instruct(:CopyBlocks, [to, from], tir_ins.location)
      end

      def on_drop(tir_ins, compiled_code, *)
        object = tir_ins.object.id

        compiled_code.instruct(:Drop, [object], tir_ins.location)
      end

      def on_move_to_pool(tir_ins, compiled_code, *)
        id = tir_ins.pool_id.id

        compiled_code.instruct(:MoveToPool, [id], tir_ins.location)
      end
    end
  end
end
