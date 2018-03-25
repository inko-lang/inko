# frozen_string_literal: true

require 'spec_helper'

describe Inkoc::TypeSystem::Database do
  let(:database) { described_class.new }

  describe '#trait_type' do
    it 'returns the Trait type from the top-level object' do
      trait = Inkoc::TypeSystem::Object.new

      database.top_level.define_attribute(Inkoc::Config::TRAIT_CONST, trait)

      expect(database.trait_type).to eq(trait)
    end
  end

  describe '#boolean_type' do
    it 'returns the Boolean type from the top-level object' do
      trait = Inkoc::TypeSystem::Object.new

      database.top_level.define_attribute(Inkoc::Config::BOOLEAN_CONST, trait)

      expect(database.boolean_type).to eq(trait)
    end
  end
end
