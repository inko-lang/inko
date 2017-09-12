# frozen_string_literal: true

module Inkoc
  module Pass
    class InsertImplicitImports
      def initialize(state)
        @state = state
      end

      def run(ast, mod)
        prepend_imports(ast, mod)

        [ast, mod]
      end

      def prepend_imports(ast, mod)
        location = ast.location
        prepend = []

        prepend << import_bootstrap(location) if mod.import_bootstrap?
        prepend << import_prelude(location) if mod.import_prelude?

        ast.prepend_nodes(prepend)
      end

      # Generates an import statement equivalent to the following:
      #
      #     import std::bootstrap::(self as _)
      def import_bootstrap(location)
        std = identifier_for(Config::STD_MODULE, location)
        bootstrap = identifier_for(Config::BOOTSTRAP_MODULE, location)
        underscore = identifier_for('_', location)

        symbol = AST::ImportSymbol
          .new(AST::Self.new(location), underscore, location)

        AST::Import.new([std, bootstrap], [symbol], location)
      end

      # Generates an import statement equivalent to the following:
      #
      #     import std::prelude::*
      def import_prelude(location)
        std = identifier_for(Config::STD_MODULE, location)
        prelude = identifier_for(Config::PRELUDE_MODULE, location)
        symbol = AST::GlobImport.new(location)

        AST::Import.new([std, prelude], [symbol], location)
      end

      def identifier_for(name, location)
        AST::Identifier.new(name, location)
      end
    end
  end
end
