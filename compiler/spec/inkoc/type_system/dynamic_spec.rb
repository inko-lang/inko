# frozen_string_literal: true

require 'spec_helper'

describe Inkoc::TypeSystem::Dynamic do
  let(:dynamic) { described_class.new }

  describe '#prototype' do
    it 'returns nil' do
      expect(dynamic.prototype).to be_nil
    end
  end

  describe '#dynamic?' do
    it 'returns true' do
      expect(dynamic).to be_dynamic
    end
  end

  describe '#attributes' do
    it 'returns a SymbolTable' do
      expect(dynamic.attributes).to be_an_instance_of(Inkoc::SymbolTable)
    end
  end

  describe '#type_parameters' do
    it 'returns a TypeParameterTable' do
      expect(dynamic.type_parameters)
        .to be_an_instance_of(Inkoc::TypeSystem::TypeParameterTable)
    end
  end

  describe '#type_parameter_instances' do
    it 'returns a TypeParameterInstances' do
      expect(dynamic.type_parameter_instances)
        .to be_an_instance_of(Inkoc::TypeSystem::TypeParameterInstances)
    end
  end

  describe '#implemented_traits' do
    it 'returns an empty Set' do
      expect(dynamic.implemented_traits).to be_empty
    end
  end

  describe '#define_attribute' do
    it 'returns a NullSymbol' do
      expect(dynamic.define_attribute('foo', dynamic))
        .to be_an_instance_of(Inkoc::NullSymbol)
    end
  end

  describe '#lookup_attribute' do
    it 'returns a NullSymbol' do
      expect(dynamic.lookup_attribute('foo'))
        .to be_an_instance_of(Inkoc::NullSymbol)
    end
  end

  describe '#lookup_method' do
    it 'returns a NullSymbol' do
      expect(dynamic.lookup_method('foo'))
        .to be_an_instance_of(Inkoc::NullSymbol)
    end
  end

  describe '#type_compatible?' do
    let(:state) { Inkoc::State.new(Inkoc::Config.new) }
    let(:ours) { described_class.new }

    context 'when comparing with another dynamic type' do
      it 'returns true' do
        theirs = described_class.new

        expect(ours.type_compatible?(theirs, state)).to eq(true)
      end
    end

    context 'when comparing with an optional dynamic type' do
      it 'returns true' do
        theirs = Inkoc::TypeSystem::Optional.new(described_class.new)

        expect(ours.type_compatible?(theirs, state)).to eq(true)
      end
    end

    context 'when comparing with a type parameter' do
      it 'returns true if the parameter does not have any requirements' do
        theirs = Inkoc::TypeSystem::TypeParameter.new(name: 'T')

        expect(ours.type_compatible?(theirs, state)).to eq(true)
      end

      it 'returns false if the parameter requires a trait' do
        trait = state.typedb.new_trait_type('A')

        theirs = Inkoc::TypeSystem::TypeParameter
          .new(name: 'T', required_traits: [trait])

        expect(ours.type_compatible?(theirs, state)).to eq(false)
      end
    end

    context 'when comparing with an object' do
      it 'returns false' do
        theirs = Inkoc::TypeSystem::Object.new(name: 'A')

        expect(ours.type_compatible?(theirs, state)).to eq(false)
      end
    end
  end

  describe '#guard_unknown_message?' do
    it 'returns true' do
      type = described_class.new

      expect(type.guard_unknown_message?('foo')).to eq(true)
    end
  end
end
