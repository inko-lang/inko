# frozen_string_literal: true

module Inkoc
  module AST
    class AssignAttribute
      include TypeOperations
      include Predicates
      include Inspect

      attr_reader :name, :value, :location

      def initialize(name, value, location)
        @name = name
        @value = value
        @location = location
      end

      def visitor_method
        :on_assign_attribute
      end
    end
  end
end
