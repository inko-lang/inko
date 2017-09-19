# frozen_string_literal: true

module Inkoc
  module TIR
    module Instruction
      class GetFloatPrototype
        include Predicates
        include Inspect

        attr_reader :register, :location

        def initialize(register, location)
          @register = register
          @location = location
        end

        def visitor_method
          :on_get_float_prototype
        end
      end
    end
  end
end
