# frozen_string_literal: true

module Inkoc
  module AST
    class DefineAttribute
      include TypeOperations
      include Predicates
      include Inspect

      attr_reader :name, :value_type, :location

      # name - The name of the attribute to define.
      # vtype - The type of the value.
      # location - The SourceLocation of the definition.
      def initialize(name, vtype, location)
        @name = name
        @value_type = vtype
        @location = location
      end

      def visitor_method
        :on_define_attribute
      end

      def define_attribute?
        true
      end
    end
  end
end
