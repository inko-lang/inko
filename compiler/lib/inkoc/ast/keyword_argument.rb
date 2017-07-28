# frozen_string_literal: true

module Inkoc
  module AST
    class KeywordArgument
      include Inspect

      attr_reader :name, :value, :location

      # name - The name of the argument.
      # value - The value the argument is set to.
      # location - The SourceLocation of the keyword.
      def initialize(name, value, location)
        @name = name
        @value = value
        @location = location
      end

      def tir_process_node_method
        :on_keyword_argument
      end
    end
  end
end
