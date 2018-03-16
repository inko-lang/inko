# frozen_string_literal: true

module Inkoc
  module AST
    class MethodRequirement
      include TypeOperations
      include Predicates
      include Inspect

      attr_reader :name, :required_traits, :location

      # name - The name of the type parameter the requirements apply to.
      # required_traits - The traits required by the type parameter.
      # location - The SourceLocation of the constraint.
      def initialize(name, required_traits, location)
        @name = name
        @required_traits = required_traits
        @location = location
      end
    end
  end
end
