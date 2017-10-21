# frozen_string_literal: true

module Inkoc
  module TIR
    module Instruction
      class SkipNextBlock
        include Predicates
        include Inspect

        attr_reader :location

        # location - The SourceLocation of this instruction.
        def initialize(location)
          @location = location
        end

        def register
          nil
        end

        def visitor_method
          :on_skip_next_block
        end
      end
    end
  end
end
