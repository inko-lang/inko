# frozen_string_literal: true

module Inkoc
  module Pass
    class AstToModule
      def initialize(state)
        @state = state
      end

      def run(ast)
        qname = qualified_name_for_path(ast.location.file.path)
        mod = TIR::Module.new(qname, ast.location)

        @state.store_module(mod)

        [ast, mod]
      end

      def qualified_name_for_path(path)
        TIR::QualifiedName.new([module_name_for_path(path)])
      end

      # Returns the module name for a file path.
      #
      # Example:
      #
      #     module_name_for_path('hello/world.inko') # => "world"
      def module_name_for_path(path)
        file = path.split(File::SEPARATOR).last

        file ? file.split('.').first : '<anonymous-module>'
      end
    end
  end
end
