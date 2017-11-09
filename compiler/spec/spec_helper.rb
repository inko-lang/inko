# frozen_string_literal: true

require 'rspec'
require_relative '../lib/inkoc'

module Inkoc
  module Tests
    FIXTURE_PATH = File.expand_path('../fixtures', __FILE__).freeze
  end
end

RSpec.configure do |c|
  c.order = :random
  c.color = true
end
