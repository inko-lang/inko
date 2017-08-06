# frozen_string_literal: true

module Inkoc
  module Type
    class Block
      include Inspect
      include ObjectOperations
      include TypeCompatibility

      attr_reader :argument_types, :type_arguments, :prototype, :attributes
      attr_accessor :rest_argument, :throws, :returns

      def initialize(prototype)
        @argument_types = []
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
    end
  end
end
