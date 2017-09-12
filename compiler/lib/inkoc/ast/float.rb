# frozen_string_literal: true

module Inkoc
  module AST
    class Float
      include Inspect

      attr_reader :value, :location

      # value - The value of the float.
      # location - The SourceLocation of the float.
      def initialize(value, location)
        @value = value
        @location = location
      end

      def visitor_method
        :on_float
      end
    end
  end
end
