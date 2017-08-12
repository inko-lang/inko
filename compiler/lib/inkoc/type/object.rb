# frozen_string_literal: true

module Inkoc
  module Type
    class Object
      include Inspect
      include ObjectOperations
      include TypeCompatibility

      attr_reader :attributes, :implemented_traits, :type_arguments
      attr_accessor :name, :prototype

      def initialize(name = nil, prototype = nil)
        @name = name
        @prototype = prototype
        @attributes = SymbolTable.new
        @implemented_traits = []
        @type_arguments = SymbolTable.new
      end

      def regular_object?
        true
      end

      def block?
        false
      end

      def lookup_type(name)
        super.or_else { @type_arguments[name] }
      end
    end
  end
end
