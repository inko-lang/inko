# frozen_string_literal: true

module Inkoc
  module AST
    class Return
      include TypeOperations
      include Predicates
      include Inspect

      attr_reader :value, :location

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
