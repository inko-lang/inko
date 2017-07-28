# frozen_string_literal: true

module Inkoc
  module AST
    class Constant
      include Inspect

      attr_reader :name, :location, :receiver
      attr_accessor :return_type, :type_arguments

      # name - The name of the constant as a String.
      # location - The SourceLocation of the constant.
      # receiver - The object to search for the constant.
      def initialize(name, receiver, location)
        @name = name
        @receiver = receiver
        @location = location
        @return_type = nil
        @type_arguments = []
      end

      def tir_process_node_method
        :on_constant
      end

      def tir_define_variable_method
        :on_define_constant
      end
    end
  end
end
