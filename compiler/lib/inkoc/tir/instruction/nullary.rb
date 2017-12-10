# frozen_string_literal: true

module Inkoc
  module TIR
    module Instruction
      class Nullary
        include Inspect
        include Predicates

        attr_reader :name, :register, :location

        def initialize(name, register, location)
          @name = name
          @register = register
          @location = location
        end

        def visitor_method
          :on_nullary
        end
      end
    end
  end
end
