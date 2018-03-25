# frozen_string_literal: true

module Inkoc
  module AST
    class Return
      include TypeOperations
      include Predicates
      include Inspect

      attr_reader :value, :location

      # value - The value to return, if any.
      # location - The SourceLocation of the return statement.
      def initialize(value, location)
        @value = value
        @location = location
      end

      def visitor_method
        :on_return
      end

      def return?
        true
      end

      def value_location
        value ? value.location : location
      end
    end
  end
end
