# frozen_string_literal: true

module Inkoc
  module Pass
    class InsertImplicitImports
      def initialize(mod, state)
        @module = mod
        @state = state
      end

      def run(ast)
        prepend_imports(ast)

        [ast]
      end

      def prepend_imports(ast)
        loc = ast.location

        @module.imports << import_bootstrap(loc) if @module.import_bootstrap?
        @module.imports << import_globals(loc) if @module.import_globals?
        @module.imports << import_prelude(loc) if @module.import_prelude?
      end

      # Generates the import statement for importing the bootstrap module.
      def import_bootstrap(location)
        import_and_ignore(Config::BOOTSTRAP_MODULE, location)
      end

      # Generates the import statement for the globals module.
      #
      # Equivalent:
      #
      #     import core::globals::*
      def import_globals(location)
        import_everything_from(Config::GLOBALS_MODULE, location)
      end

      # Generates the import statement for the prelude module.
      #
      # Equivalent:
      #
      #     import core::prelude::*
      def import_prelude(location)
        import_everything_from(Config::PRELUDE_MODULE, location)
      end

      def identifier_for(name, location)
        AST::Identifier.new(name, location)
      end

      # Imports a module without exposing it as a global.
      #
      # Equivalent:
      #
      #     import core::bootstrap::(self as _)
      def import_and_ignore(name, location)
        core = identifier_for(Config::CORE_MODULE, location)
        bootstrap = identifier_for(name, location)
        underscore = identifier_for('_', location)

        symbol = AST::ImportSymbol
          .new(AST::Self.new(location), underscore, location)

        AST::Import.new([core, bootstrap], [symbol], location)
      end

      def import_everything_from(module_name, location)
        core = identifier_for(Config::CORE_MODULE, location)
        prelude = identifier_for(module_name, location)
        symbol = AST::GlobImport.new(location)

        AST::Import.new([core, prelude], [symbol], location)
      end

      def import_std_module_as(name, symbol_name, location)
        std = identifier_for(Config::STD_MODULE, location)
        name_ident = identifier_for(name, location)
        alias_name = identifier_for(symbol_name, location)

        symbol = AST::ImportSymbol
          .new(AST::Self.new(location), alias_name, location)

        AST::Import.new([std, name_ident], [symbol], location)
      end
    end
  end
end
