# frozen_string_literal: true

module Inkoc
  module TIR
    class CatchEntry
      attr_reader :try_block, :else_block

      def initialize(try_block, else_block)
        @try_block = try_block
        @else_block = else_block
      end
    end
  end
end
