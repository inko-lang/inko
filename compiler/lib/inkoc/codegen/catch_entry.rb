# frozen_string_literal: true

module Inkoc
  module Codegen
    class CatchEntry
      attr_reader :start, :stop, :jump_to

      def initialize(start, stop, jump_to)
        @start = start
        @stop = stop
        @jump_to = jump_to
      end
    end
  end
end
