# frozen_string_literal: true

module Inkoc
  module TIR
    module Instruction
      class ArrayLength
        include Predicates
        include Inspect

        attr_reader :register, :array, :location

        def initialize(register, array, location)
          @register = register
          @array = array
          @location = location
        end

        def visitor_method
          :on_array_length
        end
      end
    end
  end
end
