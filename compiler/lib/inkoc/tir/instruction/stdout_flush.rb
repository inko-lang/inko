# frozen_string_literal: true

module Inkoc
  module TIR
    module Instruction
      class StdoutFlush
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
          :on_stdout_flush
        end
      end
    end
  end
end
