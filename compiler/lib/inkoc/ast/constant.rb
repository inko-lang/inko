# frozen_string_literal: true

module Inkoc
  module AST
    class Constant
      include TypeOperations
      include Predicates
      include Inspect

      attr_reader :name, :location, :receiver
      attr_accessor :return_type, :type_parameters, :optional

      # name - The name of the constant as a String.
      # location - The SourceLocation of the constant.
      # receiver - The object to search for the constant.
      def initialize(name, receiver, location)
        @name = name
        @receiver = receiver
        @location = location
        @return_type = nil
        @type_parameters = []
        @optional = false
      end

      def constant?
        true
      end

      def optional?
        @optional
      end

      def self_type?
        name == Config::SELF_TYPE
      end

      def dynamic_type?
        name == Config::DYNAMIC_TYPE
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
