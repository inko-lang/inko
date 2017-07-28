# frozen_string_literal: true

module Inkoc
  module AST
    class Self
      include Inspect

      attr_reader :location

      # @location = location
      def initialize(location)
        @location = location
      end

      def tir_process_node_method
        :on_self
      end
    end
  end
end
