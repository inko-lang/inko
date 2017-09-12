# frozen_string_literal: true

module Inkoc
  module AST
    class Integer
      include Inspect

      attr_reader :value, :location

      # value - The value of the integer.
      # location - The SourceLocation of the integer.
      def initialize(value, location)
        @value = value
        @location = location
      end

      def visitor_method
        :on_integer
      end
    end
  end
end
