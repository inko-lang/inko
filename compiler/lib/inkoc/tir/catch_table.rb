# frozen_string_literal: true

module Inkoc
  module TIR
    class CatchTable
      include Inspect

      def initialize
        @entries = []
      end

      def add(*args)
        @entries << CatchEntry.new(*args)
      end

      def to_a
        @entries
      end
    end
  end
end
