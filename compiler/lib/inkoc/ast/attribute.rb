# frozen_string_literal: true

module Inkoc
  module AST
    class Attribute
      include Predicates
      include Inspect

      attr_reader :name, :location

      # name - The name of the attribute.
      # location - The SourceLocation of the attribute.
      def initialize(name, location)
        @name = name
        @location = location
      end

      def visitor_method
        :on_attribute
      end

      def define_variable_visitor_method
        :on_define_attribute
      end

      def reassign_variable_visitor_method
        :on_reassign_attribute
      end
    end
  end
end
