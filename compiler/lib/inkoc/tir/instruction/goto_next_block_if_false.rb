# frozen_string_literal: true

module Inkoc
  module TIR
    module Instruction
      class GotoNextBlockIfFalse
        include Predicates
        include Inspect

        attr_reader :register, :location

        # register - The virtual register containing the condition to evaluate.
        # location - The SourceLocation of this instruction.
        def initialize(register, location)
          @register = register
          @location = location
        end

        def visitor_method
          :on_goto_next_block_if_false
        end
      end
    end
  end
end
