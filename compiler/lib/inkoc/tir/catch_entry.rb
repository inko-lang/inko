# frozen_string_literal: true

module Inkoc
  module TIR
    class CatchEntry
      include Inspect

      attr_reader :start, :stop, :jump_to, :register

      def initialize(start, stop, jump_to, register)
        @stop = stop
        @jump_to = jump_to
        @register = register
      end
    end
  end
end
