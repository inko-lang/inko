# frozen_string_literal: true

module Inkoc
  module TIR
    module Instruction
      class SetBlock
        include Predicates
        include Inspect

        attr_reader :register, :code_object, :location

        def initialize(register, code_object, location)
          @register = register
          @code_object = code_object
          @location = location
        end

        def visitor_method
          :on_set_block
        end
      end
    end
  end
end
