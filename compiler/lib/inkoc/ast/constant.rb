# frozen_string_literal: true

module Inkoc
  module AST
    class Constant
      include Predicates
      include Inspect

      attr_reader :name, :location, :receiver
      attr_accessor :return_type, :type_parameters, :required_traits, :optional

      # name - The name of the constant as a String.
      # location - The SourceLocation of the constant.
      # receiver - The object to search for the constant.
      def initialize(name, receiver, location)
        @name = name
        @receiver = receiver
        @location = location
        @return_type = nil
        @type_parameters = []
        @required_traits = []
        @optional = false
      end

      def constant?
        true
      end

      def optional?
        @optional
      end

      def visitor_method
        :on_constant
      end

      def define_variable_visitor_method
        :on_define_constant
      end
    end
  end
end
