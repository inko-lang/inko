# frozen_string_literal: true

module Inkoc
  module TIR
    module Instruction
      class GetBlockPrototype
        include Inspect

        attr_reader :register, :location

        def initialize(register, location)
          @register = register
          @location = location
        end
      end
    end
  end
end
