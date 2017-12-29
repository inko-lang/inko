# frozen_string_literal: true

module Inkoc
  # Compiler for translating Inko source files into IVM bytecode.
  class Compiler
    BASE_PASSES = [
      Pass::PathToSource,
      Pass::SourceToAst,
      Pass::AddDefaultForRestArguments,
      Pass::DesugarObject,
      Pass::ConfigureModule,
      Pass::DefineModuleType,
      Pass::TrackModule,
      Pass::InsertImplicitImports,
      Pass::CollectImports,
      Pass::AddImplicitImportSymbols,
      Pass::CompileImportedModules,
      Pass::SetupSymbolTables,
      Pass::DefineTypes,
      Pass::ValidateConstraints,
      Pass::ValidateThrow,
      Pass::OptimizeKeywordArguments,
      Pass::GenerateTir,
      Pass::DeadCode,
      Pass::TailCallElimination,
      Pass::CodeGeneration,
      Pass::CodeWriter
    ].freeze

    attr_reader :state

    def initialize(state)
      @state = state
    end

    def compile_main(path)
      name = TIR::QualifiedName.new(%w[main])
      compile(name, path)
    end

    # name - The QualifiedName of the module.
    # path - The absolute file path of the module to compile, as a Pathname.
    def compile(name, path)
      mod = module_for_name_and_path(name, path)

      passes.reduce([]) do |input, klass|
        out = klass.new(mod, @state).run(*input)

        break if out.nil? || state.diagnostics.errors?

        out
      end

      mod
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

    def module_for_name_and_path(name, path)
      location = SourceLocation.first_line(SourceFile.new(path))

      TIR::Module.new(name, location)
    end
  end
end
