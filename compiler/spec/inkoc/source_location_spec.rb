# frozen_string_literal: true

require 'spec_helper'

describe Inkoc::SourceLocation do
  describe '.first_line' do
    it 'returns a SourceLocation for the first line' do
      file = Inkoc::SourceFile.new('foo.inko')
      loc = described_class.first_line(file)

      expect(loc.line).to eq(1)
      expect(loc.column).to eq(1)
    end
  end
end
