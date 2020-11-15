# frozen_string_literal: true

module Inkoc
  module TIR
    module Instruction
      class GotoBlockIfTrue
        include Predicates
        include Inspect

        attr_reader :block_name, :register, :location

        def initialize(block_name, register, location)
          @block_name = block_name
          @register = register
          @location = location
        end

        def visitor_method
          :on_goto_block_if_true
        end
      end
    end
  end
end
