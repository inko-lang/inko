# frozen_string_literal: true

require 'spec_helper'

describe Inkoc::ModulePathsCache do
  let(:config) { Inkoc::Config.new }
  let(:cache) { described_class.new(config) }
  let(:directory) { Support::FIXTURE_PATH }

  before do
    config.add_source_directories([directory])
  end

  describe '#absolute_path_for' do
    describe 'when a module could be found' do
      it 'returns the full module path as a Pathname' do
        expected_path = Pathname.new(directory).join('foo.inko')

        expect(cache.absolute_path_for('foo.inko'))
          .to eq([expected_path, Pathname.new(directory)])
      end

      it 'caches the path' do
        first = cache.absolute_path_for('foo.inko')
        second = cache.absolute_path_for('foo.inko')

        expect(first).to be(second)
      end
    end

    describe 'when a module could not be found' do
      it 'returns nil' do
        expect(cache.absolute_path_for('kittens.inko')).to be_nil
      end
    end
  end
end
