# frozen_string_literal: true

module Inkoc
  module TIR
    module Instruction
      class Quinary
        include Inspect
        include Predicates

        attr_reader :name, :register, :one, :two, :three, :four, :five, :location

        def initialize(name, register, one, two, three, four, five, location)
          @name = name
          @register = register
          @one = one
          @two = two
          @three = three
          @four = four
          @five = five
          @location = location
        end

        def visitor_method
          :on_quinary
        end
      end
    end
  end
end
