# frozen_string_literal: true

module Inkoc
  module TIR
    module Instruction
      class SetFloat
        include Inspect

        attr_reader :register, :value, :location

        def initialize(register, value, location)
          @register = register
          @value = value
          @location = location
        end
      end
    end
  end
end
