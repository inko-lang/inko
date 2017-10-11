# frozen_string_literal: true

module Inkoc
  module TIR
    class CatchEntry
      include Inspect

      attr_reader :reason, :start, :stop, :jump_to, :register

      def self.named(name, start, stop, jump_to, register)
        new(THROW_REASONS[name], start, stop, jump_to, register)
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
