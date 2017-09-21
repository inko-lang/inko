# frozen_string_literal: true

module Inkoc
  module TIR
    class VirtualRegisters
      include Inspect

      def initialize
        @registers = []
      end

      def allocate(type)
        register = VirtualRegister.new(@registers.length, type)

        @registers << register

        register
      end

      def length
        @registers.length
      end

      def empty?
        @registers.empty?
      end
    end
  end
end
