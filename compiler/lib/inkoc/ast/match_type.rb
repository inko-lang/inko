# frozen_string_literal: true

module Inkoc
  module AST
    class MatchType
      include Predicates
      include Inspect
      include TypeOperations

      attr_reader :pattern, :guard, :body, :location

      def initialize(pattern, guard, body, location)
        @pattern = pattern
        @guard = guard
        @body = body
        @location = location
      end

      def visitor_method
        :on_match_type
      end
    end
  end
end
