# frozen_string_literal: true

module Inkoc
  module AST
    class Array
      include Inspect

      attr_reader :values, :location

      def initialize(values, location)
        @values = values
        @location = location
      end

      def tir_process_node_method
        :on_array
      end
    end
  end
end
