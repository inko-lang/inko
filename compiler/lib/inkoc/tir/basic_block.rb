# frozen_string_literal: true

module Inkoc
  module TIR
    class BasicBlock
      # The name of the basic block as a String.
      attr_reader :name

      # The block that preceded this block, if any.
      attr_accessor :previous

      # The instructions that make up this basic block.
      attr_reader :instructions

      # The next BasicBlock to execute, if any.
      attr_reader :next

      def initialize(name, next_block = nil)
        @name = name
        @previous = nil
        @instructions = []

        self.next = next_block
      end

      def location
        @instructions[0].location
      end

      def instruct(klass, *args)
        instruction = Instruction.const_get(klass).new(*args)

        @instructions << instruction

        instruction
      end

      def next=(block)
        block&.previous = self

        @next = block
      end

      def instruction_offset
        block = previous
        offset = 0

        while block
          offset += block.instructions.length

          block = block.previous
        end

        offset
      end
    end
  end
end
