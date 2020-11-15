# frozen_string_literal: true

module Inkoc
  module AST
    class MatchExpression
      include Predicates
      include Inspect
      include TypeOperations

      attr_reader :patterns, :guard, :body, :location

      def initialize(patterns, guard, body, location)
        @patterns = patterns
        @guard = guard
        @body = body
        @location = location
      end

      def visitor_method
        :on_match_expression
      end
    end
  end
end
