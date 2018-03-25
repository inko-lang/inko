# frozen_string_literal: true

require 'spec_helper'

describe Inkoc::SourceFile do
  let(:directory) { Support::FIXTURE_PATH }

  describe '#lines' do
    describe 'when the file exists' do
      it 'returns the lines of the file' do
        file = described_class.new(File.join(directory, 'foo.inko'))

        expect(file.lines).to eq(["# This is an empty module.\n"])
      end
    end

    describe 'when the file does not exist' do
      it 'returns an empty Array' do
        file = described_class.new(File.join(directory, 'kittens.inko'))

        expect(file.lines).to eq([])
      end
    end
  end
end
