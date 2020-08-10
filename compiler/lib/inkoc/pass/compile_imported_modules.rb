# frozen_string_literal: true

module Inkoc
  module Pass
    class CompileImportedModules
      def initialize(compiler, mod)
        @compiler = compiler
        @module = mod
      end

      def state
        @compiler.state
      end

      def run(ast)
        @module.imports.each { |import| on_import(import) }

        [ast]
      end

      def on_import(node)
        qname = node.qualified_name
        loc = node.location

        compile_module(qname, loc) unless state.module_exists?(qname.to_s)
      end

      def compile_module(qname, location)
        rel_path = qname.source_path_with_extension

        if (full_path = state.find_module_path(rel_path))
          @compiler.compile(qname, full_path)
        else
          state.diagnostics.module_not_found_error(qname.to_s, location)
        end
      end
    end
  end
end
