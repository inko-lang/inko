# frozen_string_literal: true

module Inkoc
  module TIR
    module Instruction
      class Return
        include Predicates
        include Inspect

        attr_reader :block_return, :register, :location

        def initialize(block_return, register, location)
          @block_return = block_return
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
