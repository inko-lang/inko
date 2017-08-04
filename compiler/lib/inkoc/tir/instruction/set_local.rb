# frozen_string_literal: true

module Inkoc
  module TIR
    module Instruction
      class SetLocal
        include Inspect

        attr_reader :variable, :value, :location

        def initialize(variable, value, location)
          @variable = variable
          @value = value
          @location = location
        end

        def register
          value
        end
      end
    end
  end
end
