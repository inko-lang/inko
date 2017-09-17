# frozen_string_literal: true

module Inkoc
  module AST
    class Pair
      include Predicates
      include Inspect

      attr_reader :key, :value, :location

      def initialize(key, value, location)
        @key = key
        @value = value
        @location = location
      end
    end
  end
end
