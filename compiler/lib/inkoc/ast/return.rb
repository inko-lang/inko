# frozen_string_literal: true

module Inkoc
  module AST
    class Return
      include TypeOperations
      include Predicates
      include Inspect

      attr_reader :value, :location, :local

      def initialize(value, local, location)
        @value = value
        @local = local
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
