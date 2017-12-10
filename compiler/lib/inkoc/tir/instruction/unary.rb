# frozen_string_literal: true

module Inkoc
  module TIR
    module Instruction
      class Unary
        include Inspect
        include Predicates

        attr_reader :name, :register, :operand, :location

        def initialize(name, register, operand, location)
          @name = name
          @register = register
          @operand = operand
          @location = location
        end

        def visitor_method
          :on_unary
        end
      end
    end
  end
end
