# frozen_string_literal: true

module Inkoc
  module TIR
    module Instruction
      class CopyBlocks
        include Inspect
        include Predicates

        attr_reader :to, :from, :location

        def initialize(to, from, location)
          @to = to
          @from = from
          @location = location
        end

        def visitor_method
          :on_copy_blocks
        end

        def register
          from
        end
      end
    end
  end
end
