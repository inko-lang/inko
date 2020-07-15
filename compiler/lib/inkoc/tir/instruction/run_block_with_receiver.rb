# frozen_string_literal: true

module Inkoc
  module TIR
    module Instruction
      class RunBlockWithReceiver
        include Predicates
        include Inspect

        attr_reader :block, :start, :amount, :location, :block_type, :receiver

        def initialize(block, receiver, start, amount, block_type, location)
          @block = block
          @receiver = receiver
          @start = start
          @amount = amount
          @block_type = block_type
          @location = location
        end

        def register
          nil
        end

        def run_block?
          true
        end

        def visitor_method
          :on_run_block_with_receiver
        end
      end
    end
  end
end
