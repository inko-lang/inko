# frozen_string_literal: true

module Inkoc
  module Codegen
    class CatchEntry
      attr_reader :start, :stop, :jump_to, :register

      def initialize(start, stop, jump_to, register)
        @start = start
        @stop = stop
        @jump_to = jump_to
        @register = register
      end
    end
  end
end
