# frozen_string_literal: true

module Inkoc
  module Type
    class Trait
      include Inspect
      include ObjectOperations
      include TypeCompatibility

      attr_reader :name, :attributes, :required_methods, :type_arguments,
                  :prototype

      def initialize(name, prototype = nil)
        @name = name
        @attributes = SymbolTable.new
        @type_arguments = SymbolTable.new
        @prototype = prototype
        @required_methods = SymbolTable.new
      end

      def trait?
        true
      end

      def lookup_type(name)
        super.or_else { @type_arguments[name] }
      end
    end
  end
end
