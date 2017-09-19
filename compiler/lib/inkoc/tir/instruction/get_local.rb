# frozen_string_literal: true

module Inkoc
  module TIR
    module Instruction
      class GetLocal
        include Predicates
        include Inspect

        attr_reader :register, :variable, :location

        def initialize(register, variable, location)
          @register = register
          @variable = variable
          @location = location
        end

        def visitor_method
          :on_get_local
        end
      end
    end
  end
end
