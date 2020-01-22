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
        @module.imports << import_prelude(loc) if @module.import_prelude?
      end

      # Generates the import statement for importing the bootstrap module.
      def import_bootstrap(location)
        import_everything_from(Config::BOOTSTRAP_MODULE, location)
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

      def import_everything_from(module_name, location)
        core = identifier_for(Config::CORE_MODULE, location)
        prelude = identifier_for(module_name, location)
        symbol = AST::GlobImport.new(location)

        AST::Import.new([core, prelude], [symbol], location)
      end
    end
  end
end
