# frozen_string_literal: true

require 'spec_helper'

describe Inkoc::TypeScope do
  let(:self_type) { Inkoc::TypeSystem::Object.new }
  let(:block_type) { Inkoc::TypeSystem::Block.new }
  let(:module_type) { Inkoc::TypeSystem::Object.new }

  describe '#lookup_type' do
    let(:type_scope) do
      described_class
        .new(self_type, block_type, module_type, locals: Inkoc::SymbolTable.new)
    end

    it 'can look up a type from the block of the scope' do
      type = Inkoc::TypeSystem::Object.new

      block_type.define_attribute('A', type)

      expect(type_scope.lookup_type('A')).to eq(type)
    end

    it 'can look up a type from the "self" type' do
      type = Inkoc::TypeSystem::Object.new

      self_type.define_attribute('A', type)

      expect(type_scope.lookup_type('A')).to eq(type)
    end

    it 'can look up a type from the module type' do
      type = Inkoc::TypeSystem::Object.new

      module_type.define_attribute('A', type)

      expect(type_scope.lookup_type('A')).to eq(type)
    end

    it 'returns nil if a type does not exist' do
      expect(type_scope.lookup_type('A')).to be_nil
    end
  end

  describe '#lookup_constant' do
    let(:type_scope) do
      described_class
        .new(self_type, block_type, module_type, locals: Inkoc::SymbolTable.new)
    end

    it 'can look up a type from the block of the scope' do
      type = Inkoc::TypeSystem::Object.new

      block_type.define_attribute('A', type)

      expect(type_scope.lookup_constant('A')).to eq(type)
    end

    it 'can look up a type from the "self" type' do
      type = Inkoc::TypeSystem::Object.new

      self_type.define_attribute('A', type)

      expect(type_scope.lookup_constant('A')).to eq(type)
    end

    it 'can look up a type from the module type' do
      type = Inkoc::TypeSystem::Object.new

      module_type.define_attribute('A', type)

      expect(type_scope.lookup_constant('A')).to eq(type)
    end

    it 'returns nil if a type does not exist' do
      expect(type_scope.lookup_constant('A')).to be_nil
    end
  end
end
