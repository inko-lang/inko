# frozen_string_literal: true

module Inkoc
  module Type
    class Dynamic
      include Inspect
      include ObjectOperations

      attr_reader :attributes, :implemented_traits, :type_arguments

      attr_accessor :name, :prototype

      def initialize
        @name = 'Dynamic'
        @attributes = SymbolTable.new
        @implemented_traits = []
        @type_arguments = SymbolTable.new
        @prototype = nil
      end

      def block?
        false
      end

      def responds_to_message?(*)
        true
      end

      # Dynamic types are compatible with everything else.
      def type_compatible?(*)
        true
      end
    end
  end
end
