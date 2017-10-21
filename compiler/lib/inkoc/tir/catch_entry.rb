# frozen_string_literal: true

module Inkoc
  module TIR
    class CatchEntry
      attr_reader :try_block, :else_block, :register

      def initialize(try_block, else_block, register)
        @try_block = try_block
        @else_block = else_block
        @register = register
      end

      def inspect
        "CatchEntry(register: #{register.inspect}, ...)"
      end
    end
  end
end
