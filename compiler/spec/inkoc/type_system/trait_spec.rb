# frozen_string_literal: true

require 'spec_helper'

describe Inkoc::TypeSystem::Trait do
  describe '#trait?' do
    it 'returns true' do
      trait = described_class.new

      expect(trait).to be_trait
    end
  end

  describe '#lookup_method' do
    let(:trait) { described_class.new(unique_id: 1) }

    it 'supports looking up a method defined on the trait' do
      method = Inkoc::TypeSystem::Object.new

      trait.define_attribute('foo', method)

      symbol = trait.lookup_method('foo')

      expect(symbol.name).to eq('foo')
      expect(symbol.type).to eq(method)
    end

    it 'supports looking up a method from the required methods' do
      method = Inkoc::TypeSystem::Object.new

      trait.required_methods.define('foo', method)

      symbol = trait.lookup_method('foo')

      expect(symbol.name).to eq('foo')
      expect(symbol.type).to eq(method)
    end

    it 'supports looking up a method from the required traits' do
      method = Inkoc::TypeSystem::Object.new

      required_trait = described_class.new(unique_id: 2)
      required_trait.required_methods.define('foo', method)

      trait.add_required_trait(required_trait)

      symbol = trait.lookup_method('foo')

      expect(symbol.name).to eq('foo')
      expect(symbol.type).to eq(method)
    end

    it 'returns a NullSymbol when a method can not be found' do
      expect(trait.lookup_method('foo')).to be_an_instance_of(Inkoc::NullSymbol)
    end
  end

  describe '#type_compatible?' do
    let(:ours) { described_class.new }
    let(:state) { Inkoc::State.new(Inkoc::Config.new) }

    context 'when comparing with a Dynamic' do
      it 'returns true' do
        dynamic = Inkoc::TypeSystem::Dynamic.new

        expect(ours.type_compatible?(dynamic, state)).to eq(true)
      end
    end

    context 'when comparing with the trait itself' do
      it 'returns true' do
        expect(ours.type_compatible?(ours, state)).to eq(true)
      end
    end

    context 'when comparing with an optional type' do
      it 'returns true if the types are copmatible' do
        theirs = Inkoc::TypeSystem::Optional.new(ours)

        expect(ours.type_compatible?(theirs, state)).to eq(true)
      end
    end

    context 'when comparing with another trait' do
      let(:theirs) { described_class.new(unique_id: 2) }

      it 'returns true if the trait is a required trait' do
        ours.add_required_trait(theirs.new_instance)

        expect(ours.type_compatible?(theirs, state)).to eq(true)
      end

      it 'returns true if the trait type instance is a required trait' do
        ours.add_required_trait(theirs.new_instance)

        expect(ours.type_compatible?(theirs.new_instance, state)).to eq(true)
      end

      it 'returns false if the trait is not a required trait' do
        expect(ours.type_compatible?(theirs, state)).to eq(false)
      end

      it 'returns true when both traits are compatible generic traits' do
        ours.define_type_parameter('T')

        init_param = Inkoc::TypeSystem::TypeParameter.new(name: 'In')
        theirs = ours.new_instance([init_param])

        expect(ours.type_compatible?(theirs, state)).to eq(true)
      end

      it 'returns true if the trait is a required trait of a required trait' do
        intermediate = described_class.new(unique_id: 3)

        intermediate.add_required_trait(theirs)

        ours.add_required_trait(intermediate)

        expect(ours.type_compatible?(theirs, state)).to eq(true)
      end
    end

    context 'when comparing with a type parameter' do
      it 'returns true if the parameter defines us as a required traits' do
        theirs = Inkoc::TypeSystem::TypeParameter
          .new(name: 'T', required_traits: [ours])

        expect(ours.type_compatible?(theirs, state)).to eq(true)
      end

      it 'returns true if the type parameter does not have any requirements' do
        theirs = Inkoc::TypeSystem::TypeParameter.new(name: 'T')

        expect(ours.type_compatible?(theirs, state)).to eq(true)
      end

      it 'returns false if the parameter does not define us as required' do
        inspect = described_class.new(name: 'Inspect')

        theirs = Inkoc::TypeSystem::TypeParameter
          .new(name: 'T', required_traits: [inspect])

        expect(ours.type_compatible?(theirs, state)).to eq(false)
      end
    end

    context 'when comparing with a regular object' do
      it 'returns true if the object is in our prototype chain' do
        object_type = Inkoc::TypeSystem::Object.new
        trait_type = described_class.new(name: 'Trait', prototype: object_type)

        ours.prototype = trait_type

        expect(ours.type_compatible?(object_type, state)).to eq(true)
      end

      it 'returns false if the object is not in our prototype chain' do
        theirs = Inkoc::TypeSystem::Object.new

        expect(ours.type_compatible?(theirs, state)).to eq(false)
      end
    end
  end

  describe '#define_required_method' do
    it 'defines a required method' do
      trait = described_class.new
      method = Inkoc::TypeSystem::Block.new(name: 'foo')

      trait.define_required_method(method)

      expect(trait.required_methods['foo'].type).to eq(method)
    end
  end
end
