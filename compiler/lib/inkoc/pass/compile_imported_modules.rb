# frozen_string_literal: true

module Inkoc
  module Pass
    class CompileImportedModules
      include VisitorMethods

      def initialize(mod, state)
        @module = mod
        @state = state
      end

      def run(ast)
        process_node(ast)

        [ast]
      end

      def on_body(node)
        process_nodes(node.expressions)
      end

      def on_import(node)
        qname = node.qualified_name
        loc = node.location

        compile_module(qname, loc) unless @state.module_exists?(qname.to_s)
      end

      def compile_module(qname, location)
        rel_path = qname.source_path_with_extension

        if (full_path = @state.find_module_path(rel_path))
          Compiler.new(@state).compile(qname, full_path)
        else
          @state.diagnostics.module_not_found_error(qname.to_s, location)
        end
      end
    end
  end
end
