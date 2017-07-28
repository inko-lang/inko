# frozen_string_literal: true

module Inkoc
  module Type
    class Trait
      include Inspect
      include ObjectOperations
      include TypeCompatibility

      attr_reader :name, :attributes, :type_arguments, :prototype

      def initialize(name, prototype = nil)
        @name = name
        @attributes = SymbolTable.new
        @methods = SymbolTable.new
        @type_arguments = SymbolTable.new
        @prototype = prototype
      end

      def block?
        false
      end
    end
  end
end
