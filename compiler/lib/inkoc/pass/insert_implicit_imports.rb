# frozen_string_literal: true

module Inkoc
  module Pass
    class InsertImplicitImports
      def initialize(compiler, mod)
        @module = mod
        @state = compiler.state
      end

      def run(ast)
        prepend_imports(ast)

        [ast]
      end

      def prepend_imports(ast)
        loc = ast.location

        import_bootstrap(loc) if @module.import_bootstrap?
        import_init(loc) if @module.import_init?
        import_implicits(loc) if @module.import_implicits?
      end

      def import_bootstrap(location)
        import = import_everything_from(Config::BOOTSTRAP_MODULE, location)

        @module.imports << import
      end

      def import_init(location)
        std = identifier_for(Config::STD_MODULE, location)
        init = identifier_for(Config::INIT_MODULE, location)

        @module.imports << AST::Import.new([std, init], [], location)
      end

      def import_implicits(location)
        std = identifier_for(Config::STD_MODULE, location)

        Config::PRELUDE_SYMBOLS.each do |modname, symbol_names|
          mod = identifier_for(modname, location)
          symbols = symbol_names.map do |name|
            AST::ImportSymbol
              .new(AST::Constant.new(name, location), nil, location)
          end

          @module.imports << AST::Import.new([std, mod], symbols, location)
        end
      end

      def identifier_for(name, location)
        AST::Identifier.new(name, location)
      end

      def import_everything_from(module_name, location)
        std = identifier_for(Config::STD_MODULE, location)
        mod = identifier_for(module_name, location)
        symbol = AST::GlobImport.new(location)

        AST::Import.new([std, mod], [symbol], location)
      end
    end
  end
end
