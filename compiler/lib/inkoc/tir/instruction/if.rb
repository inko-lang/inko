# frozen_string_literal: true

module Inkoc
  module TIR
    module Instruction
      class If
        include Inspect

        attr_reader :register, :true_instructions, :false_instructions,
                    :location

        def initialize(register, true_ins, false_ins, location)
          @register = register
          @true_instructions = true_ins
          @false_instructions = false_ins
          @location = location
        end
      end
    end
  end
end
