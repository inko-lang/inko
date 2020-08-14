# frozen_string_literal: true

module Inkoc
  # Compiler for translating Inko source files into IVM bytecode.
  class Compiler
    BASE_PASSES = [
      Pass::PathToSource,
      Pass::SourceToAst,
      Pass::DesugarObject,
      Pass::DesugarMethod,
      Pass::DefineModuleType,
      Pass::TrackModule,
      Pass::InsertImplicitImports,
      Pass::CollectImports,
      Pass::AddImplicitImportSymbols,
      Pass::CompileImportedModules,
      Pass::SetupSymbolTables,
      Pass::DefineThisModuleType,
      Pass::DefineImportTypes,
      Pass::DefineTypeSignatures,
      Pass::ImplementTraits,
      Pass::DefineType,
      Pass::ValidateThrow,
      Pass::GenerateTir,
      Pass::TailCallElimination,
      Pass::CodeGeneration,
    ].freeze

    attr_reader :state, :modules

    def initialize(state)
      @state = state
      @modules = []
    end

    def compile_main(path)
      name = TIR::QualifiedName.new(%w[main])
      main_mod = compile(name, path)

      Codegen::Serializer.new(self, main_mod).serialize_to_file

      main_mod
    end

    # name - The QualifiedName of the module.
    # path - The absolute file path of the module to compile, as a Pathname.
    def compile(name, path)
      mod = module_for_name_and_path(name, path)
      output = passes.reduce([]) do |input, klass|
        out = klass.new(self, mod).run(*input)

        break if out.nil? || state.diagnostics.errors?

        out
      end

      @modules.push(output.first) if output

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
