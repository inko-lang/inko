# frozen_string_literal: true

module Inkoc
  module TIR
    module Instruction
      class ArraySet
        include Predicates
        include Inspect

        attr_reader :register, :array, :index, :value, :location

        def initialize(register, array, index, value, location)
          @register = register
          @array = array
          @index = index
          @value = value
          @location = location
        end

        def visitor_method
          :on_array_set
        end
      end
    end
  end
end
