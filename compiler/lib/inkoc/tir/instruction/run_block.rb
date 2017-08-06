# frozen_string_literal: true

module Inkoc
  module TIR
    module Instruction
      class RunBlock
        include Inspect

        attr_reader :register, :block, :arguments, :location

        def initialize(register, block, arguments, location)
          @register = register
          @block = block
          @arguments = arguments
          @location = location
        end
      end
    end
  end
end
