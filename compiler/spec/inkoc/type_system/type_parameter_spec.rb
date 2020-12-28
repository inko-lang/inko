# frozen_string_literal: true

require 'spec_helper'

describe Inkoc::TypeSystem::TypeParameter do
  let(:state) { Inkoc::State.new(Inkoc::Config.new) }

  describe '#type_name' do
    context 'without any required traits' do
      it 'returns the type name' do
        param = described_class.new(name: 'T')

        expect(param.type_name).to eq('T')
      end
    end

    context 'with one required trait' do
      it 'returns the type name' do
        trait = Inkoc::TypeSystem::Trait.new(name: 'Foo', unique_id: 1)
        param = described_class.new(name: 'T', required_traits: [trait])

        expect(param.type_name).to eq('T: Foo')
      end
    end

    context 'with multiple required traits' do
      it 'returns the type name' do
        trait1 = Inkoc::TypeSystem::Trait.new(name: 'Foo', unique_id: 1)
        trait2 = Inkoc::TypeSystem::Trait.new(name: 'Bar', unique_id: 2)
        param = described_class
          .new(name: 'T', required_traits: [trait1, trait2])

        expect(param.type_name).to eq('T: Foo + Bar')
      end
    end
  end

  describe '#type_compatible?' do
    it 'returns true when comparing with an empty type parameter' do
      ours = described_class.new(name: 'A')
      theirs = described_class.new(name: 'B')

      expect(ours.type_compatible?(theirs, state)).to eq(true)
    end

    it 'returns true when comparing with the same object' do
      ours = described_class.new(name: 'A')

      expect(ours.type_compatible?(ours, state)).to eq(true)
    end

    it 'returns true when we include all the traits of the other parameter' do
      trait1 = Inkoc::TypeSystem::Trait.new(name: 'T1', unique_id: 1)
      trait2 = Inkoc::TypeSystem::Trait.new(name: 'T2', unique_id: 2)

      ours = described_class.new(name: 'A', required_traits: [trait1, trait2])
      theirs = described_class.new(name: 'B', required_traits: [trait1])

      expect(ours.type_compatible?(theirs, state)).to eq(true)
    end

    it 'returns true if a trait is a required trait of a required trait' do
      theirs = Inkoc::TypeSystem::Trait.new(unique_id: 1)

      intermediate = Inkoc::TypeSystem::Trait.new(unique_id: 2)
      intermediate.add_required_trait(theirs)

      ours = described_class.new(required_traits: [intermediate])

      expect(ours.type_compatible?(theirs, state)).to eq(true)
    end

    it 'returns true when comparing with a trait that we include' do
      trait = Inkoc::TypeSystem::Trait.new(name: 'T1')
      ours = described_class.new(name: 'A', required_traits: [trait])

      expect(ours.type_compatible?(trait, state)).to eq(true)
    end

    it 'returns false when we are not compatible with another type' do
      ours = described_class.new(name: 'A')
      theirs = Inkoc::TypeSystem::Object.new

      expect(ours.type_compatible?(theirs, state)).to eq(false)
    end

    # rubocop: disable Metrics/LineLength
    it 'returns true when using an object that is in the prototype chain of all traits' do
      object = Inkoc::TypeSystem::Object.new(name: 'Root')
      trait = Inkoc::TypeSystem::Trait.new(name: 'Inspect', prototype: object)
      ours = described_class.new(name: 'A', required_traits: [trait])

      expect(ours.type_compatible?(object, state)).to eq(true)
    end
    # rubocop: enable Metrics/LineLength
  end

  describe '#initialize_as' do
    let(:block_type) do
      Inkoc::TypeSystem::Block.new(name: 'foo').tap do |block|
        block.define_type_parameter('T')
      end
    end

    let(:self_type) do
      Inkoc::TypeSystem::Object.new(name: 'A').tap do |object|
        object.define_type_parameter('X')
      end
    end

    it 'initialises a type parameter in a block' do
      param = block_type.lookup_type_parameter('T')
      type = Inkoc::TypeSystem::Object.new(name: 'A')

      param.initialize_as(type, block_type, self_type)

      expect(block_type.lookup_type_parameter_instance(param)).to eq(type)
      expect(self_type.lookup_type_parameter_instance(param)).to be_nil
    end

    it 'initialises a type parameter in the self type' do
      param = self_type.lookup_type_parameter('X')
      type = Inkoc::TypeSystem::Object.new(name: 'A')

      param.initialize_as(type, block_type, self_type)

      expect(block_type.lookup_type_parameter_instance(param)).to be_nil
      expect(self_type.lookup_type_parameter_instance(param)).to eq(type)
    end

    it 'does not initialise a parameter from a different type' do
      param = described_class.new(name: 'T')
      type = Inkoc::TypeSystem::Object.new(name: 'A')

      param.initialize_as(type, block_type, self_type)

      expect(block_type.lookup_type_parameter_instance(param)).to be_nil
      expect(self_type.lookup_type_parameter_instance(param)).to be_nil
    end

    it 'does not initialise an already initialised self type parameter' do
      param = self_type.lookup_type_parameter('X')
      type = Inkoc::TypeSystem::Object.new(name: 'A')
      other_type = Inkoc::TypeSystem::Object.new(name: 'B')

      param.initialize_as(type, block_type, self_type)
      param.initialize_as(other_type, block_type, self_type)

      expect(self_type.lookup_type_parameter_instance(param)).to eq(type)
    end

    it 'does not initialise an already initialised method type parameter' do
      param = block_type.lookup_type_parameter('T')
      type = Inkoc::TypeSystem::Object.new(name: 'A')
      other_type = Inkoc::TypeSystem::Object.new(name: 'B')

      param.initialize_as(type, block_type, self_type)
      param.initialize_as(other_type, block_type, self_type)

      expect(block_type.lookup_type_parameter_instance(param)).to eq(type)
    end
  end

  describe '#remap_using_method_bounds' do
    it 'remaps a type parameter using a method bound' do
      block = Inkoc::TypeSystem::Block.new
      param = described_class.new(name: 'T')
      bound = block.method_bounds.define('T')

      expect(param.remap_using_method_bounds(block)).to eq(bound)
    end
  end

  describe '#empty?' do
    it 'returns true for a type parameter without any required traits' do
      param = described_class.new(name: 'T')

      expect(param).to be_empty
    end

    it 'returns false for a type parameter with a required trait' do
      trait = Inkoc::TypeSystem::Trait.new
      param = described_class.new(name: 'T', required_traits: [trait])

      expect(param).not_to be_empty
    end
  end

  describe '#lookup_type_parameter_instance' do
    it 'returns nil' do
      param = described_class.new(name: 'T')

      expect(param.lookup_type_parameter_instance(param)).to be_nil
    end
  end

  describe '#resolve_type_parameter_with_self' do
    let(:self_type) { Inkoc::TypeSystem::Object.new }

    it 'returns the instance of the type parameter if available' do
      block = Inkoc::TypeSystem::Block.new
      param = block.define_type_parameter('T')
      instance = state.typedb.integer_type

      block.initialize_type_parameter(param, instance)

      expect(param.resolve_type_parameter_with_self(self_type, block))
        .to eq(instance)
    end

    it 'returns the instance of a type parameter initialised in self' do
      block = Inkoc::TypeSystem::Block.new
      param = block.define_type_parameter('T')
      instance = state.typedb.integer_type

      self_type.initialize_type_parameter(param, instance)

      expect(param.resolve_type_parameter_with_self(self_type, block))
        .to eq(instance)
    end

    it 'returns the type parameter if it is not initialised' do
      block = Inkoc::TypeSystem::Block.new
      param = block.define_type_parameter('T')

      expect(param.resolve_type_parameter_with_self(self_type, block))
        .to eq(param)
    end
  end
end
