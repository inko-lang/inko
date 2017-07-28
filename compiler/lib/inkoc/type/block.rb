# frozen_string_literal: true

module Inkoc
  module Type
    class Block
      include Inspect
      include ObjectOperations
      include TypeCompatibility

      attr_reader :name, :arguments, :type_arguments, :throw_type,
                  :return_type, :prototype, :attributes

      def initialize(prototype)
        @name = nil
        @arguments = []
        @rest_argument = false
        @type_arguments = SymbolTable.new
        @throw_type = nil
        @return_type = nil
        @prototype = prototype
        @attributes = SymbolTable.new
      end

      def rest?
        @rest_argument
      end

      def block?
        true
      end
    end
  end
end
