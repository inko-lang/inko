# frozen_string_literal: true

module Inkoc
  module TIR
    module Instruction
      class GeneratorAllocate
        include Predicates
        include Inspect

        attr_reader :register, :block, :start, :amount, :location, :receiver

        def initialize(register, block, receiver, start, amount, location)
          @register = register
          @block = block
          @receiver = receiver
          @start = start
          @amount = amount
          @location = location
        end

        def visitor_method
          :on_generator_allocate
        end
      end
    end
  end
end
