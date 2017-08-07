# frozen_string_literal: true

module Inkoc
  module AST
    class Identifier
      include Inspect

      attr_reader :name, :location

      # name - The name of the identifier.
      # location - The SourceLocation of the identifier.
      def initialize(name, location)
        @name = name
        @location = location
      end

      def tir_process_node_method
        :on_identifier
      end

      def tir_define_variable_method
        :on_define_local
      end

      def tir_reassign_variable_method
        :on_reassign_local
      end
    end
  end
end
