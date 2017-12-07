# frozen_string_literal: true

module Inkoc
  module TIR
    class CatchTable
      attr_reader :entries

      def initialize
        @entries = []
        @jump_targets = Set.new
      end

      def add_entry(*args)
        entry = CatchEntry.new(*args)

        @entries << entry
        @jump_targets << entry
      end

      def jump_to?(block)
        @jump_targets.include?(block)
      end
    end
  end
end
