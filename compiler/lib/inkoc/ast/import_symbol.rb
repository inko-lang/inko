# frozen_string_literal: true

module Inkoc
  module AST
    class ImportSymbol
      include Inspect

      attr_reader :symbol_name, :alias_name, :location

      def initialize(symbol_name, alias_name, location)
        @symbol_name = symbol_name
        @alias_name = alias_name
        @location = location
      end

      def import_as
        @alias_name || @symbol_name
      end
    end
  end
end
