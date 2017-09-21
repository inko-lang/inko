# frozen_string_literal: true

module Inkoc
  module TIR
    module Instruction
      class Throw
        include Predicates
        include Inspect

        attr_reader :register, :reason, :location

        def initialize(register, reason, location)
          @reason = reason
          @register = register
          @location = location
        end

        def visitor_method
          :on_throw
        end
      end
    end
  end
end
