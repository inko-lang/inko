# frozen_string_literal: true

module Inkoc
  module TIR
    module Instruction
      class IntegerAdd
        include Predicates
        include Inspect

        attr_reader :register, :base, :add, :location

        def initialize(register, base, add, location)
          @register = register
          @base = base
          @add = add
          @location = location
        end

        def visitor_method
          :on_integer_add
        end
      end
    end
  end
end
