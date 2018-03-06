# frozen_string_literal: true

require 'spec_helper'

describe Inkoc::Type::Dynamic do
  let(:type) { described_class.new }

  describe '#name' do
    it 'returns the name of the type' do
      expect(type.name).to eq('Dynamic')
    end
  end

  describe '#prototype' do
    it 'returns nil' do
      expect(type.prototype).to be_nil
    end
  end

  describe '#attributes' do
    it 'returns an empty symbol table' do
      expect(type.attributes).to be_an_instance_of(Inkoc::SymbolTable)
      expect(type.attributes).to be_empty
    end
  end

  describe '#implemented_traits' do
    it 'returns an empty Set' do
      expect(type.implemented_traits).to be_an_instance_of(Set)
      expect(type.implemented_traits).to be_empty
    end
  end

  describe '#type_parameters' do
    it 'returns an empty type parameter table' do
      expect(type.type_parameters)
        .to be_an_instance_of(Inkoc::Type::TypeParameterTable)

      expect(type.type_parameters).to be_empty
    end
  end

  describe '#new_shallow_instance' do
    it 'returns the instance itself' do
      expect(type.new_shallow_instance).to be(type)
    end
  end

  describe '#responds_to_message?' do
    it 'returns true' do
      expect(type.responds_to_message?('foo')).to eq(true)
    end
  end

  describe '#lookup_attribute' do
    it 'returns a NullSymbol' do
      expect(type.lookup_attribute('foo'))
        .to be_an_instance_of(Inkoc::NullSymbol)
    end
  end

  describe '#type_compatible?' do
    it 'returns true when comparing with another dynamic type' do
      expect(type.type_compatible?(described_class.new)).to eq(true)
    end

    it 'returns false when comparing with any other type' do
      block = Inkoc::Type::Block.new

      expect(type.type_compatible?(block)).to eq(false)
    end
  end

  describe '#dynamic?' do
    it 'returns true' do
      expect(type.dynamic?).to eq(true)
    end
  end

  describe '#regular_object?' do
    it 'returns true' do
      expect(type.regular_object?).to eq(true)
    end
  end

  describe '#implementation_of?' do
    it 'returns false' do
      block = Inkoc::Type::Block.new

      expect(type.implementation_of?(block)).to eq(false)
    end
  end

  describe '#==' do
    it 'returns true when comparing with another dynamic type' do
      expect(type).to eq(described_class.new)
    end
  end

  describe '#type_name' do
    it 'returns the type name' do
      expect(type.type_name).to eq('Dynamic')
    end
  end
end
