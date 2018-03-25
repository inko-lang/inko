# frozen_string_literal: true

require 'spec_helper'

describe Inkoc::TypeSystem::TypeParameterInstances do
  let(:instances) { described_class.new }
  let(:param) { Inkoc::TypeSystem::TypeParameter.new(name: 'T') }

  describe '#[]' do
    it 'returns Nil when a parameter is not initialised' do
      expect(instances[param]).to be_nil
    end

    it 'returns the instance of a parameter that is initialised' do
      instance = Inkoc::TypeSystem::Object.new

      instances.define(param, instance)

      expect(instances[param]).to eq(instance)
    end
  end

  describe '#define' do
    it 'defines a type parameter instance' do
      instance = Inkoc::TypeSystem::Object.new

      expect(instances.define(param, instance)).to eq(instance)
    end
  end

  describe '#empty?' do
    it 'returns true for an empty list of instances' do
      expect(instances).to be_empty
    end

    it 'returns false for a non-empty list of instances' do
      instance = Inkoc::TypeSystem::Object.new

      instances.define(param, instance)

      expect(instances).not_to be_empty
    end
  end

  describe '#==' do
    it 'returns true when two tables are the same' do
      other = described_class.new

      expect(instances).to eq(other)
    end

    it 'returns false when two tables are not the same' do
      instances.define(param, Inkoc::TypeSystem::Object.new)

      expect(instances).not_to eq(described_class.new)
    end
  end

  describe '#dup' do
    it 'returns a copy' do
      instance1 = Inkoc::TypeSystem::Object.new(name: 'A')
      instance2 = Inkoc::TypeSystem::Object.new(name: 'B')

      instances.define(param, instance1)

      copy = instances.dup

      copy.define(param, instance2)

      expect(instances[param]).to eq(instance1)
      expect(copy[param]).to eq(instance2)
    end
  end
end
