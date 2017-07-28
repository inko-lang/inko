# frozen_string_literal: true

module Inkoc
  module Type
    class Object
      include Inspect
      include ObjectOperations
      include TypeCompatibility

      attr_reader :name, :attributes, :implemented_traits, :type_arguments

      attr_accessor :prototype

      def initialize(name = nil)
        @name = name
        @attributes = SymbolTable.new
        @implemented_traits = []
        @type_arguments = SymbolTable.new
        @prototype = nil
      end

      def block?
        false
      end
    end
  end
end
