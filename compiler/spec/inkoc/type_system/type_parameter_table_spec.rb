# frozen_string_literal: true

require 'spec_helper'

describe Inkoc::TypeSystem::TypeParameterTable do
  let(:table) { described_class.new }

  describe '#define' do
    it 'defines a type parameter' do
      trait = Inkoc::TypeSystem::Trait.new
      param = table.define('T', [trait])

      expect(param).to be_an_instance_of(Inkoc::TypeSystem::TypeParameter)
      expect(param.name).to eq('T')
      expect(param.required_traits).to eq([trait].to_set)
    end
  end

  describe '#[]' do
    context 'when using an existing parameter name' do
      it 'returns the type parameter' do
        trait = Inkoc::TypeSystem::Trait.new
        param = table.define('T', [trait])

        expect(table['T']).to eq(param)
      end
    end

    context 'when using a non-existing parameter name' do
      it 'returns nil' do
        expect(table['T']).to be_nil
      end
    end
  end

  describe '#at_index' do
    context 'when using an existing parameter index' do
      it 'returns the type parameter' do
        trait = Inkoc::TypeSystem::Trait.new
        param = table.define('T', [trait])

        expect(table.at_index(0)).to eq(param)
      end
    end

    context 'when using a non-existing parameter index' do
      it 'returns nil' do
        expect(table.at_index(0)).to be_nil
      end
    end
  end

  describe '#each' do
    it 'yields every type parameter to the supplied block' do
      trait = Inkoc::TypeSystem::Trait.new
      param = table.define('T', [trait])

      expect { |b| table.each(&b) }.to yield_with_args(param)
    end
  end

  describe '#empty?' do
    it 'returns true for an empty table' do
      expect(table).to be_empty
    end

    it 'returns false for a non-empty table' do
      trait = Inkoc::TypeSystem::Trait.new

      table.define('T', [trait])

      expect(table).not_to be_empty
    end
  end

  describe '#any?' do
    it 'returns true for a non-empty table' do
      trait = Inkoc::TypeSystem::Trait.new

      table.define('T', [trait])

      expect(table).to be_any
    end

    it 'returns false for an empty table' do
      expect(table).not_to be_any
    end
  end

  describe '#length' do
    it 'returns the number of type parameters' do
      table.define('T')

      expect(table.length).to eq(1)
    end
  end

  describe '#defines?' do
    it 'returns true for a parameter defined by the table' do
      param = table.define('T')

      expect(table.defines?(param)).to eq(true)
    end

    it 'returns false for a parameter defined by another table' do
      other_table = described_class.new

      other_table.define('T')
      param = table.define('T')

      expect(other_table.defines?(param)).to eq(false)
    end
  end

  describe '#to_a' do
    it 'returns an array of type parameters' do
      param = table.define('T')

      expect(table.to_a).to eq([param])
    end
  end
end
