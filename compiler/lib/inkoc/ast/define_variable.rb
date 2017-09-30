# frozen_string_literal: true

module Inkoc
  module AST
    class DefineVariable
      include TypeOperations
      include Predicates
      include Inspect

      attr_reader :variable, :value, :value_type, :location

      # var - The variable to define.
      # value - The value of the variable.
      # vtype - The type of the value.
      # mutable - Set to `true` for mutable variable definitions.
      # location - The SourceLocation of the definition.
      def initialize(var, value, vtype, mutable, location)
        @variable = var
        @value = value
        @value_type = vtype
        @mutable = mutable
        @location = location
      end

      def mutable?
        @mutable
      end

      def visitor_method
        :on_define_variable
      end

      def variable_definition?
        true
      end
    end
  end
end
