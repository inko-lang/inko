# frozen_string_literal: true

module Inkoc
  module Type
    class Block
      include Inspect
      include ObjectOperations
      include TypeCompatibility

      attr_reader :arguments, :type_arguments, :prototype, :attributes
      attr_accessor :rest_argument, :throws, :returns

      def initialize(prototype)
        @arguments = SymbolTable.new
        @rest_argument = false
        @type_arguments = SymbolTable.new
        @throws = nil
        @returns = nil
        @prototype = prototype
        @attributes = SymbolTable.new
      end

      def block?
        true
      end

      def return_type
        returns
      end

      def lookup_argument(name)
        @arguments[name]
      end

      def lookup_type(name)
        super.or_else { @type_arguments[name] }
      end
    end
  end
end
