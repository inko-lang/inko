# frozen_string_literal: true

module Inkoc
  module AST
    class Match
      include Predicates
      include Inspect
      include TypeOperations

      attr_accessor :bind_to_symbol
      attr_reader :expression, :bind_to, :arms, :match_else, :location

      def initialize(expression, bind_to, arms, match_else, location)
        @expression = expression
        @bind_to = bind_to
        @arms = arms
        @match_else = match_else
        @location = location
        @bind_to_symbol = nil
      end

      def visitor_method
        :on_match
      end
    end
  end
end
