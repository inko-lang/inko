# frozen_string_literal: true

module Inkoc
  module AST
    class String
      include TypeOperations
      include Predicates
      include Inspect

      attr_reader :value, :location

      # value - The value of the string.
      # location - The SourceLocation of the string.
      def initialize(value, location)
        @value = value
        @location = location
      end

      def string?
        true
      end

      def visitor_method
        :on_string
      end
    end
  end
end
