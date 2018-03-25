# frozen_string_literal: true

module Inkoc
  module AST
    class DefineTypeParameter
      include TypeOperations
      include Predicates
      include Inspect

      attr_reader :name, :location
      attr_accessor :required_traits

      def initialize(name, location)
        @name = name
        @location = location
        @required_traits = []
      end

      def type_name
        name
      end

      def visitor_method
        :on_define_type_parameter
      end
    end
  end
end
