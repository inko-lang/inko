# frozen_string_literal: true

module Inkoc
  module AST
    class CoalesceNil
      include TypeOperations
      include Predicates
      include Inspect

      attr_reader :expression, :default, :location

      def initialize(expression, default, location)
        @expression = expression
        @default = default
        @location = location
      end

      def visitor_method
        :on_coalesce_nil
      end
    end
  end
end
