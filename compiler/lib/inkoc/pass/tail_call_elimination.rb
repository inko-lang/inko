# frozen_string_literal: true

module Inkoc
  module Pass
    class TailCallElimination
      def initialize(mod, state)
        @module = mod
        @state = state
      end

      def run
        on_code_object(@module.body)

        []
      end

      def on_code_object(code_object)
        block = code_object.each_reachable_basic_block.to_a.last

        on_basic_block(code_object, block)

        code_object.code_objects.each { |code| on_code_object(code) }
      end

      def on_basic_block(code, block)
        # The last instruction is always a Return instruction, so we check the
        # instruction that preceeds it.
        index = -2
        ins = block.instructions[index]

        return unless ins

        if ins.move_result?
          ins = block.instructions[index = -3]
        end

        return unless tail_call?(code, ins)

        block.instructions[index] =
          TIR::Instruction::TailCall.new(ins.start, ins.amount, ins.location)
      end

      def diagnostics
        @state.diagnostics
      end

      def tail_call?(code, instruction)
        instruction.run_block? && instruction.block_type == code.type
      end
    end
  end
end
