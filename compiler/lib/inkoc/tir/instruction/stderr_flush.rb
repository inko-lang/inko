# frozen_string_literal: true

module Inkoc
  module TIR
    module Instruction
      class StderrFlush
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
          :on_stderr_flush
        end
      end
    end
  end
end
