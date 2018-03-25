# frozen_string_literal: true

require 'spec_helper'

describe Inkoc::Pass::DefineThisModuleType do
  describe '#run' do
    it 'defines the type of the ThisModule global' do
      mod = new_tir_module
      mod.type = Inkoc::TypeSystem::Object.new
      ast = Inkoc::AST::Body.new([], mod.location)

      expect(described_class.new(mod).run(ast)).to eq([ast])
      expect(mod.globals[Inkoc::Config::MODULE_GLOBAL].type).to eq(mod.type)
    end
  end
end
