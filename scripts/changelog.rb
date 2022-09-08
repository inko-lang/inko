#!/usr/bin/env ruby
# frozen_string_literal: true

require 'json'
require 'net/http'
require 'uri'

version = ARGV.fetch(0)
uri =
  URI.parse('https://gitlab.com/api/v4/projects/4545016/repository/changelog')
uri.query = URI.encode_www_form(version: version)
resp = Net::HTTP.get_response(uri)

unless resp.is_a?(Net::HTTPSuccess)
  abort "Failed to get the changelog: #{resp.body}"
end

section = JSON.load(resp.body).fetch('notes')
file = File.expand_path('../CHANGELOG.md', __dir__)
changelog = File.read(file)
marker = "<!-- new section -->\n"

File.open(file, 'w') do |handle|
  handle.write(changelog.gsub(marker, "#{marker}\n#{section}"))
end
