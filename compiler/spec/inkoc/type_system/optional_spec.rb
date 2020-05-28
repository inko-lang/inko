# frozen_string_literal: true

require 'spec_helper'

describe Inkoc::TypeSystem::Optional do
  describe '.wrap' do
    it 'wraps a type in an Optional' do
      object = Inkoc::TypeSystem::Object.new
      optional = described_class.wrap(object)

      expect(optional).to be_an_instance_of(described_class)
    end

    it 'does not wrap an Optional' do
      object = Inkoc::TypeSystem::Object.new
      opt1 = described_class.wrap(object)
      opt2 = described_class.wrap(opt1)

      expect(opt2.type).to eq(object)
    end
  end

  describe '#new_instance' do
    it 'returns an Optional' do
      object = Inkoc::TypeSystem::Object.new
      optional = described_class.wrap(object)

      expect(optional.new_instance).to be_an_instance_of(described_class)
    end
  end

  describe '#lookup_method' do
    it 'looks up a method in the wrapped type' do
      object = Inkoc::TypeSystem::Object.new
      method = Inkoc::TypeSystem::Object.new
      optional = described_class.wrap(object)

      object.define_attribute('foo', method)

      symbol = optional.lookup_method('foo')

      expect(symbol).to be_an_instance_of(Inkoc::Symbol)
      expect(symbol.type).to eq(method)
    end
  end

  describe '#type_name' do
    it 'returns the type name' do
      object = Inkoc::TypeSystem::Object.new(name: 'A')
      optional = described_class.new(object)

      expect(optional.type_name).to eq('?A')
    end
  end

  describe '#type_compatible?' do
    let(:state) { Inkoc::State.new(Inkoc::Config.new) }

    it 'returns true when the underlying type is compatible' do
      trait = Inkoc::TypeSystem::Trait.new(name: 'Inspect')
      object = Inkoc::TypeSystem::Object.new

      object.implement_trait(trait)

      ours = described_class.new(object)
      theirs = described_class.new(trait)

      expect(ours.type_compatible?(theirs, state)).to eq(true)
    end

    it 'returns false when passing an optional to a non-optional' do
      trait = Inkoc::TypeSystem::Trait.new(name: 'Inspect')
      object = Inkoc::TypeSystem::Object.new
      state = Inkoc::State.new(Inkoc::Config.new)

      object.implement_trait(trait)

      ours = described_class.new(object)
      theirs = trait

      expect(ours.type_compatible?(theirs, state)).to eq(false)
    end

    it 'returns true when passing to a type parameter' do
      type = Inkoc::TypeSystem::Object.new
      ours = described_class.new(type)
      theirs = Inkoc::TypeSystem::TypeParameter.new(name: 'A')

      expect(ours.type_compatible?(theirs, state)).to eq(true)
    end
  end

  describe '#generic_type?' do
    it 'returns false if the underlying type is not a generic type' do
      option = described_class
        .new(Inkoc::TypeSystem::TypeParameter.new(name: 'T'))

      expect(option).not_to be_generic_type
    end

    it 'returns true if the underlying type is a generic type' do
      type = Inkoc::TypeSystem::Object.new(name: 'A')

      type.define_type_parameter('T')

      option = described_class.new(type)

      expect(option).to be_generic_type
    end
  end

  describe '#guard_unknown_message?' do
    it 'returns true' do
      type = described_class.new(Inkoc::TypeSystem::Object.new(name: 'A'))

      expect(type.guard_unknown_message?('foo')).to eq(true)
    end
  end
end
