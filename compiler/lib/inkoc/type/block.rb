# frozen_string_literal: true

module Inkoc
  module Type
    class Block
      include Inspect
      include ObjectOperations
      include TypeCompatibility

      attr_reader :name, :arguments, :type_parameters, :prototype, :attributes
      attr_accessor :rest_argument, :throws, :returns

      def initialize(prototype, name: nil)
        @name = name
        @prototype = prototype
        @arguments = SymbolTable.new
        @rest_argument = false
        @type_parameters = {}
        @throws = nil
        @returns = nil
        @attributes = SymbolTable.new
      end

      def block?
        true
      end

      def return_type
        returns
      end

      def define_type_parameter(name, param)
        @type_parameters[name] = param
      end

      def lookup_argument(name)
        @arguments[name]
      end

      def lookup_type(name)
        symbol = lookup_attribute(name)

        return symbol.type if symbol.any?

        type_parameters[name]
      end
    end
  end
end
