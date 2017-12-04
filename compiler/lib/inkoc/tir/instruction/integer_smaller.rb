# frozen_string_literal: true

module Inkoc
  module TIR
    module Instruction
      class IntegerSmaller
        include Predicates
        include Inspect

        attr_reader :register, :base, :other, :location

        def initialize(register, base, other, location)
          @register = register
          @base = base
          @other = other
          @location = location
        end

        def visitor_method
          :on_integer_smaller
        end
      end
    end
  end
end
