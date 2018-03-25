# frozen_string_literal: true

require 'spec_helper'

describe Inkoc::TypeSystem::Void do
  let(:state) { Inkoc::State.new(Inkoc::Config.new) }

  describe '#type_compatible?' do
    let(:ours) { described_class.new }

    it 'returns true when comparing with a dynamic type' do
      theirs = Inkoc::TypeSystem::Dynamic.new

      expect(ours.type_compatible?(theirs, state)).to eq(true)
    end

    it 'returns true when comparing with another void type' do
      theirs = described_class.new

      expect(ours.type_compatible?(theirs, state)).to eq(true)
    end

    it 'returns true when comparing with a type parameter' do
      theirs = Inkoc::TypeSystem::TypeParameter.new(name: 'T')

      expect(ours.type_compatible?(theirs, state)).to eq(true)
    end

    it 'returns true when comparing with an optional void type' do
      theirs = Inkoc::TypeSystem::Optional.new(described_class.new)

      expect(ours.type_compatible?(theirs, state)).to eq(true)
    end

    it 'returns true when comparing with an optional dynamic type' do
      theirs = Inkoc::TypeSystem::Optional.new(Inkoc::TypeSystem::Dynamic.new)

      expect(ours.type_compatible?(theirs, state)).to eq(true)
    end

    it 'returns true when comparing with an optional type parameter' do
      theirs = Inkoc::TypeSystem::Optional
        .new(Inkoc::TypeSystem::TypeParameter.new(name: 'T'))

      expect(ours.type_compatible?(theirs, state)).to eq(true)
    end
  end
end
