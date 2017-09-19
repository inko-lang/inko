# frozen_string_literal: true

module Inkoc
  module TIR
    class CatchEntry
      include Inspect

      REASONS = { return: 0, throw: 1 }.freeze

      attr_reader :reason, :start, :stop, :jump_to, :register

      def self.named(name, start, stop, jump_to, register)
        new(REASONS[name], start, stop, jump_to, register)
      end

      def initialize(reason, start, stop, jump_to, register)
        @reason = reason
        @start = start
        @stop = stop
        @jump_to = jump_to
        @register = register
      end
    end
  end
end
