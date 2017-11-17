# frozen_string_literal: true

module Inkoc
  module AST
    class ImportSymbol
      include TypeOperations
      include Predicates
      include Inspect

      attr_reader :symbol_name, :alias_name, :location

      def initialize(symbol_name, alias_name, location)
        @symbol_name = symbol_name
        @alias_name = alias_name
        @location = location
      end

      def location_for_name
        alias_name ? alias_name.location : symbol_name.location
      end

      def import_as(source_module)
        if symbol_name.self?
          import_self_as(source_module)
        else
          (alias_name || symbol_name).name
        end
      end

      def import_self_as(source_module)
        if alias_name
          alias_name.name
        else
          source_module.name.module_name
        end
      end

      def expose?
        return true unless alias_name

        alias_name.name != '_'
      end

      def visitor_method
        if symbol_name.self?
          :on_import_self
        else
          :on_import_symbol
        end
      end
    end
  end
end
