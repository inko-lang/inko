# frozen_string_literal: true

module Inkoc
  module TIR
    class BasicBlock
      include Inspect

      # The name of the basic block as a String.
      attr_reader :name

      # All blocks that may jump to this block.
      attr_reader :callers

      # The instructions that make up this basic block.
      attr_reader :instructions

      # The next BasicBlock to execute, if any.
      attr_reader :next

      def initialize(name, next_block = nil)
        @name = name
        @callers = []
        @instructions = []

        self.next = next_block
      end

      def location
        @instructions[0].location
      end

      def instruct(klass, *args)
        @instructions << Instruction.const_get(klass).new(*args)
      end

      def next=(block)
        block.callers << self if block
        @next = block
      end
    end
  end
end
