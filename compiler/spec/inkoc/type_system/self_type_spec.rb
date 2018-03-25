# frozen_string_literal: true

require 'spec_helper'

describe Inkoc::TypeSystem::SelfType do
  describe '#resolve_self_type' do
    it 'resolves a Self type to a concrete type' do
      self_type = described_class.new
      target_type = Inkoc::TypeSystem::Object.new

      expect(self_type.resolve_self_type(target_type)).to eq(target_type)
    end
  end

  describe '#type_name' do
    it 'returns the type name' do
      expect(described_class.new.type_name).to eq('Self')
    end
  end
end
