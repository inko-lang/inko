# frozen_string_literal: true

module Inkoc
  module AST
    class Identifier
      include Predicates
      include Inspect

      attr_reader :name, :location

      # name - The name of the identifier.
      # location - The SourceLocation of the identifier.
      def initialize(name, location)
        @name = name
        @location = location
      end

      def identifier?
        true
      end

      def visitor_method
        :on_identifier
      end

      def define_variable_visitor_method
        :on_define_local
      end

      def reassign_variable_visitor_method
        :on_reassign_local
      end
    end
  end
end
