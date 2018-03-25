# frozen_string_literal: true

require 'spec_helper'

describe Inkoc::Equality do
  describe '#==' do
    let(:dummy) do
      Class.new do
        include Inkoc::Equality

        def initialize(number)
          @number = number
        end
      end
    end

    it 'returns true when two objects are the same' do
      ours = dummy.new(10)
      theirs = dummy.new(10)

      expect(ours).to eq(theirs)
    end

    it 'returns false when two objects are not the same' do
      ours = dummy.new(10)
      theirs = dummy.new(20)

      expect(ours).not_to eq(theirs)
    end
  end
end
