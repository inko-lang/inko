# frozen_string_literal: true

module Inkoc
  module AST
    class HashMap
      include Inspect

      attr_reader :pairs, :location

      def initialize(pairs, location)
        @pairs = pairs
        @location = location
      end

      def tir_process_node_method
        :on_hash_map
      end
    end
  end
end
