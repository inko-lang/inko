# frozen_string_literal: true

module Inkoc
  module TIR
    module Instruction
      class Return
        include Predicates
        include Inspect

        attr_reader :register, :location

        def initialize(register, location)
          @register = register
          @location = location
        end

        def return?
          true
        end

        def visitor_method
          :on_return
        end
      end
    end
  end
end
