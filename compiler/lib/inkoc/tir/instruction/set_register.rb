# frozen_string_literal: true

module Inkoc
  module TIR
    module Instruction
      class SetRegister
        include Predicates
        include Inspect

        attr_reader :register, :source_register, :location

        def initialize(register, source_register, location)
          @register = register
          @source_register = source_register
          @location = location
        end

        def visitor_method
          :on_set_register
        end
      end
    end
  end
end
