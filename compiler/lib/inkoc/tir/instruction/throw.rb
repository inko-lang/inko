# frozen_string_literal: true

module Inkoc
  module TIR
    module Instruction
      class Throw
        include Predicates
        include Inspect

        attr_reader :method_throw, :register, :location

        def initialize(method_throw, register, location)
          @method_throw = method_throw
          @register = register
          @location = location
        end

        def return?
          true
        end

        def visitor_method
          :on_throw
        end
      end
    end
  end
end
