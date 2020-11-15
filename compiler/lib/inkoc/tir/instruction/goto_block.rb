# frozen_string_literal: true

module Inkoc
  module TIR
    module Instruction
      class GotoBlock
        include Predicates
        include Inspect

        attr_reader :block_name, :location

        def initialize(block_name, location)
          @block_name = block_name
          @location = location
        end

        def register
          nil
        end

        def visitor_method
          :on_goto_block
        end
      end
    end
  end
end
