# frozen_string_literal: true

require 'spec_helper'

describe Inkoc::TypeSystem::Never do
  let(:state) { Inkoc::State.new(Inkoc::Config.new) }

  describe '#type_compatible?' do
    let(:ours) { described_class.new }

    it 'returns true when comparing with another never type' do
      theirs = described_class.new

      expect(ours.type_compatible?(theirs, state)).to eq(true)
    end

    it 'returns true when comparing with a type parameter' do
      theirs = Inkoc::TypeSystem::TypeParameter.new(name: 'T')

      expect(ours.type_compatible?(theirs, state)).to eq(true)
    end

    it 'returns true when comparing with an optional never type' do
      theirs = Inkoc::TypeSystem::Optional.new(described_class.new)

      expect(ours.type_compatible?(theirs, state)).to eq(true)
    end

    it 'returns true when comparing with an optional type parameter' do
      theirs = Inkoc::TypeSystem::Optional
        .new(Inkoc::TypeSystem::TypeParameter.new(name: 'T'))

      expect(ours.type_compatible?(theirs, state)).to eq(true)
    end
  end
end
