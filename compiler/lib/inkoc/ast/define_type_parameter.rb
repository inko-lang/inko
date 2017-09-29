# frozen_string_literal: true

module Inkoc
  module AST
    class DefineTypeParameter
      include TypeOperations
      include Predicates
      include Inspect

      attr_reader :name, :required_traits, :location

      def initialize(name, location)
        @name = name
        @location = location
        @required_traits = []
      end
    end
  end
end
