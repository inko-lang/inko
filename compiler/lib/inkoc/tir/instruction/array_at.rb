# frozen_string_literal: true

module Inkoc
  module TIR
    module Instruction
      class ArrayAt
        include Predicates
        include Inspect

        attr_reader :register, :array, :index, :location

        def initialize(register, array, index, location)
          @register = register
          @array = array
          @index = index
          @location = location
        end

        def visitor_method
          :on_array_at
        end
      end
    end
  end
end
