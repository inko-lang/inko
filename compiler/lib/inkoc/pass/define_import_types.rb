# frozen_string_literal: true

module Inkoc
  module Pass
    # Pass that defines the types for all imported symbols.
    class DefineImportTypes
      include VisitorMethods
      include TypePass

      def run(ast)
        process_imports

        [ast]
      end

      def process_imports
        @module.imports.each do |node|
          process_node(node)
        end
      end

      def on_import(node)
        name = node.qualified_name
        imported_module = @state.module(name)

        node.symbols.each do |symbol|
          process_node(symbol, imported_module) if symbol.expose?
        end
      end

      def on_import_self(symbol, source_mod)
        import_as = symbol.import_as(source_mod)
        location = symbol.location_for_name

        import_symbol(import_as, source_mod.type, location)
      end

      def on_import_symbol(symbol, source_mod)
        source_name = symbol.symbol_name.name
        source_sym = source_mod.lookup_attribute(source_name)
        import_as = symbol.import_as(source_mod)

        if source_sym.any?
          import_symbol(import_as, source_sym.type, symbol.location_for_name)
        else
          diagnostics.import_undefined_symbol_error(
            source_mod.name,
            source_name,
            symbol.location
          )
        end
      end

      def on_import_glob(symbol, source_mod)
        location = symbol.location_for_name

        source_mod.attributes.mapping.each do |name, attribute|
          import_symbol(name, attribute.type, location)
        end
      end

      def import_symbol(name, type, location)
        if @module.global_defined?(name)
          diagnostics.import_existing_symbol_error(name, location)
        elsif type.method? && type.extern
          diagnostics.external_function_import(name, location)
        else
          @module.globals.define(name, type)
        end
      end
    end
  end
end
