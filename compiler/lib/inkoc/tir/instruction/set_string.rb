# frozen_string_literal: true

module Inkoc
  module TIR
    module Instruction
      class SetString
        include Predicates
        include Inspect

        attr_reader :register, :value, :location

        def initialize(register, value, location)
          @register = register
          @value = value
          @location = location
        end

        def visitor_method
          :on_set_literal
        end
      end
    end
  end
end
