# frozen_string_literal: true

module Inkoc
  module Type
    class HashMap
      include Inspect
      include ObjectOperations
      include TypeCompatibility

      attr_reader :prototype, :attributes, :implemented_traits, :type_arguments

      def initialize(prototype)
        @prototype = prototype
        @attributes = SymbolTable.new
        @implemented_traits = []
        @type_arguments = SymbolTable.new
      end
    end
  end
end
