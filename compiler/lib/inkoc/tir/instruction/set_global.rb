# frozen_string_literal: true

module Inkoc
  module TIR
    module Instruction
      class SetGlobal
        include Predicates
        include Inspect

        attr_reader :register, :variable, :value, :location

        def initialize(register, variable, value, location)
          @register = register
          @variable = variable
          @value = value
          @location = location
        end
      end
    end
  end
end
