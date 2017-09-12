# frozen_string_literal: true

module Inkoc
  module AST
    class ReassignVariable
      include Inspect

      attr_reader :variable, :value, :location

      # var - The variable to re-assign.
      # value - The new value.
      # location - The SourceLocation of the re-assignment.
      def initialize(var, value, location)
        @variable = var
        @value = value
        @location = location
      end

      def visitor_method
        :on_reassign_variable
      end
    end
  end
end
