# frozen_string_literal: true

module Inkoc
  module AST
    class MatchElse
      include Predicates
      include Inspect
      include TypeOperations

      attr_reader :body, :location

      def initialize(body, location)
        @body = body
        @location = location
      end

      def visitor_method
        :on_match_else
      end
    end
  end
end
