# frozen_string_literal: true

module Inkoc
  module TIR
    module Instruction
      class RunBlock
        include Predicates
        include Inspect

        attr_reader :register, :block, :arguments, :location

        def initialize(register, block, arguments, location)
          @register = register
          @block = block
          @arguments = arguments
          @location = location
        end

        def visitor_method
          :on_run_block
        end
      end
    end
  end
end
