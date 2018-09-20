# frozen_string_literal: true

module Inkoc
  module TIR
    module Instruction
      class Quaternary
        include Inspect
        include Predicates

        attr_reader :name, :register, :one, :two, :three, :four, :location

        def initialize(name, register, one, two, three, four, location)
          @name = name
          @register = register
          @one = one
          @two = two
          @three = three
          @four = four
          @location = location
        end

        def visitor_method
          :on_quaternary
        end
      end
    end
  end
end
