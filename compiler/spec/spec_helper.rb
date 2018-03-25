# frozen_string_literal: true

require 'rspec'
require_relative '../lib/inkoc'
require_relative 'support/fixture_path'
require_relative 'support/tir_module'
require_relative 'support/parser'

RSpec.configure do |c|
  c.order = :random
  c.color = true
  c.include Support::TirModule
  c.include Support::Parser
end
