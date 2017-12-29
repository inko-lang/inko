# frozen_string_literal: true

module Inkoc
  module TIR
    module Instruction
      class Ternary
        include Inspect
        include Predicates

        attr_reader :name, :register, :one, :two, :three, :location

        def initialize(name, register, one, two, three, location)
          @name = name
          @register = register
          @one = one
          @two = two
          @three = three
          @location = location
        end

        def visitor_method
          :on_ternary
        end
      end
    end
  end
end
