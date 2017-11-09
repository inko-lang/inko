# frozen_string_literal: true

require 'spec_helper'

describe Inkoc::Diagnostic do
  let(:source_file) { Inkoc::SourceFile.new('foo.inko') }
  let(:source_location) { Inkoc::SourceLocation.new(1, 2, source_file) }

  describe '.error' do
    it 'returns an error diagnostic' do
      diag = described_class.error('foo', source_location)

      expect(diag.level).to eq(:error)
    end
  end

  describe '.warning' do
    it 'returns a warning diagnostic' do
      diag = described_class.warning('foo', source_location)

      expect(diag.level).to eq(:warning)
    end
  end

  describe '#error?' do
    it 'returns true for an error' do
      diag = described_class.error('foo', source_location)

      expect(diag.error?).to eq(true)
    end

    it 'returns false for a warning' do
      diag = described_class.warning('foo', source_location)

      expect(diag.error?).to eq(false)
    end
  end

  describe '#line' do
    it 'returns the line number' do
      diag = described_class.error('foo', source_location)

      expect(diag.line).to eq(1)
    end
  end

  describe '#column' do
    it 'returns the column number' do
      diag = described_class.error('foo', source_location)

      expect(diag.column).to eq(2)
    end
  end

  describe '#file' do
    it 'returns the source file' do
      diag = described_class.error('foo', source_location)

      expect(diag.file).to eq(source_file)
    end
  end

  describe '#path' do
    it 'returns the file path as a Pathname' do
      diag = described_class.error('foo', source_location)

      expect(diag.path).to eq(Pathname.new('foo.inko'))
    end
  end
end
