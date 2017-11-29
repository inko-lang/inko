# frozen_string_literal: true

module Inkoc
  module TIR
    module Instruction
      class ArrayInsert
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
          :on_array_insert
        end
      end
    end
  end
end
