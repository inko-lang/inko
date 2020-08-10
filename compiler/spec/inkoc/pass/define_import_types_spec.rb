# frozen_string_literal: true

require 'spec_helper'

describe Inkoc::Pass::DefineImportTypes do
  let(:state) { Inkoc::State.new(Inkoc::Config.new) }
  let(:tir_module) { new_tir_module }
  let(:pass) { described_class.new(Inkoc::Compiler.new(state), tir_module) }
  let(:ast) { Inkoc::AST::Body.new([], tir_module.location) }

  describe '#run' do
    it 'processes all the imports and returns the AST' do
      allow(pass).to receive(:process_imports)

      expect(pass.run(ast)).to eq([ast])

      expect(pass).to have_received(:process_imports)
    end
  end

  describe '#process_imports' do
    it 'processes every import' do
      import = Inkoc::AST::Import.new([], [], tir_module.location)
      tir_module.imports << import

      allow(pass)
        .to receive(:on_import)
        .with(import)

      pass.run([ast])

      expect(pass).to have_received(:on_import)
    end
  end

  describe '#on_import' do
    let(:time_module) { instance_double('std::time') }

    before do
      allow(state)
        .to receive(:module)
        .with(an_instance_of(Inkoc::TIR::QualifiedName))
        .and_return(time_module)
    end

    it 'imports every symbol' do
      import = parse_source('import std::time::(self)').expressions[0]

      allow(pass)
        .to receive(:on_import_self)
        .with(import.symbols[0], time_module)

      pass.on_import(import)

      expect(pass).to have_received(:on_import_self)
    end

    it 'does not import a symbol that should not be exposed' do
      import = parse_source('import std::time::(self as _)').expressions[0]

      allow(pass).to receive(:on_import_self)

      pass.on_import(import)

      expect(pass).not_to have_received(:on_import_self)
    end
  end

  describe '#on_import_self' do
    it 'imports the module as a global' do
      import_symbol = parse_source('import rspec::(self)')
        .expressions[0]
        .symbols[0]

      imported_mod = new_tir_module
      imported_mod.type = Inkoc::TypeSystem::Object.new

      pass.on_import_self(import_symbol, imported_mod)

      symbol = tir_module.globals['rspec']

      expect(symbol).to be_an_instance_of(Inkoc::Symbol)
      expect(symbol.type).to eq(imported_mod.type)
    end
  end

  describe '#on_import_symbol' do
    let(:imported_module) do
      new_tir_module.tap do |mod|
        mod.type = Inkoc::TypeSystem::Object.new
      end
    end

    context 'when using an existing symbol' do
      it 'imports the symbol as a global' do
        import_symbol = parse_source('import rspec::(number)')
          .expressions[0]
          .symbols[0]

        number_type = Inkoc::TypeSystem::Object.new

        imported_module.type.attributes.define('number', number_type)

        pass.on_import_symbol(import_symbol, imported_module)

        symbol = tir_module.globals['number']

        expect(symbol).to be_an_instance_of(Inkoc::Symbol)
        expect(symbol.type).to eq(number_type)
      end

      it 'supports aliasing of the imported symbol' do
        import_symbol = parse_source('import rspec::(number as foo)')
          .expressions[0]
          .symbols[0]

        number_type = Inkoc::TypeSystem::Object.new

        imported_module.type.attributes.define('number', number_type)

        pass.on_import_symbol(import_symbol, imported_module)

        symbol = tir_module.globals['foo']

        expect(symbol).to be_an_instance_of(Inkoc::Symbol)
        expect(symbol.type).to eq(number_type)
      end
    end

    context 'when using a non-existing symbol' do
      it 'produces an undefined symbol error' do
        import_symbol = parse_source('import rspec::(number)')
          .expressions[0]
          .symbols[0]

        pass.on_import_symbol(import_symbol, imported_module)

        expect(state.diagnostics.errors?).to eq(true)
      end
    end
  end

  describe '#on_import_glob' do
    it 'imports all the symbols from a module' do
      imported_module = new_tir_module.tap do |mod|
        mod.type = Inkoc::TypeSystem::Object.new
      end

      import_symbol = Inkoc::AST::GlobImport.new(tir_module.location)
      number_type = Inkoc::TypeSystem::Object.new

      imported_module.type.attributes.define('number', number_type)

      pass.on_import_glob(import_symbol, imported_module)

      symbol = tir_module.globals['number']

      expect(symbol).to be_an_instance_of(Inkoc::Symbol)
      expect(symbol.type).to eq(number_type)
    end
  end
end
