# frozen_string_literal: true

module Inkoc
  module TIR
    module Instruction
      class ExternalFunctionCall
        include Predicates
        include Inspect

        attr_reader :register, :function, :start, :amount, :location

        def initialize(register, function, start, amount, location)
          @register = register
          @function = function
          @start = start
          @amount = amount
          @location = location
        end

        def visitor_method
          :on_external_function_call
        end
      end
    end
  end
end
