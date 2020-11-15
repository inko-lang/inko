# frozen_string_literal: true

module Inkoc
  module AST
    class Throw
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
        :on_throw
      end

      def throw?
        true
      end
    end
  end
end
