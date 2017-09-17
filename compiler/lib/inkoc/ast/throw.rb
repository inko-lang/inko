# frozen_string_literal: true

module Inkoc
  module AST
    class Throw
      include Predicates
      include Inspect

      attr_reader :value, :location

      # value - The value to throw
      # location - The SourceLocation of the throw statement.
      def initialize(value, location)
        @value = value
        @location = location
      end

      def visitor_method
        :on_throw
      end
    end
  end
end
