# frozen_string_literal: true

module Inkoc
  module AST
    class Attribute
      include Inspect

      attr_reader :name, :location

      # name - The name of the attribute.
      # location - The SourceLocation of the attribute.
      def initialize(name, location)
        @name = name
        @location = location
      end

      def tir_process_node_method
        :on_attribute
      end

      def tir_define_variable_method
        :on_define_attribute
      end

      def tir_reassign_variable_method
        :on_reassign_attribute
      end
    end
  end
end
