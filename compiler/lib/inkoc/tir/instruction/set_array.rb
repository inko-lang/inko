# frozen_string_literal: true

module Inkoc
  module TIR
    module Instruction
      class SetArray
        include Predicates
        include Inspect

        attr_reader :register, :values, :location

        def initialize(register, values, location)
          @register = register
          @values = values
          @location = location
        end

        def visitor_method
          :on_set_array
        end
      end
    end
  end
end
