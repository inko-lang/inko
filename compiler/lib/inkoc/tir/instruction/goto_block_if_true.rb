# frozen_string_literal: true

module Inkoc
  module TIR
    module Instruction
      class GotoBlockIfTrue
        include Predicates
        include Inspect

        attr_reader :register, :block, :location

        # register - The virtual register containing the condition to evaluate.
        # block - The block to jump to.
        # location - The SourceLocation of this instruction.
        def initialize(register, block, location)
          @register = register
          @block = block
          @location = location
        end

        def visitor_method
          :on_goto_block_if_true
        end
      end
    end
  end
end
