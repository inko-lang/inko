# frozen_string_literal: true

module Inkoc
  module TIR
    module Instruction
      class GetTrue
        include Predicates
        include Inspect

        attr_reader :register, :location

        def initialize(register, location)
          @register = register
          @location = location
        end

        def visitor_method
          :on_get_true
        end
      end
    end
  end
end
