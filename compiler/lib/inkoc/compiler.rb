# frozen_string_literal: true

module Inkoc
  # Compiler for translating Inko source files into IVM bytecode.
  class Compiler
    BASE_PASSES = [
      Pass::PathToSource,
      Pass::SourceToAst,
      Pass::InsertImplicitImports,
      Pass::AstToModule,
      Pass::CompileImportedModules,
      Pass::DefineTypes,
      Pass::ModuleBody
    ].freeze

    def initialize(state)
      @state = state
    end

    # path - The absolute file path of the module to compile, as a String.
    def compile(path)
      out = passes.reduce([path]) do |input, klass|
        out = klass.new(@state).run(*input)

        out ? out : break
      end
    end

    def passes
      if @state.config.release_mode?
        passes_for_release_mode
      else
        passes_for_debug_mode
      end
    end

    def passes_for_debug_mode
      BASE_PASSES
    end

    def passes_for_release_mode
      BASE_PASSES
    end
  end
end
