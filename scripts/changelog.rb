#!/usr/bin/env ruby
# frozen_string_literal: true

require 'optparse'
require 'time'

def commits_in(directories, from, to)
  command = "git log #{from}..#{to} --format='* %h: %s' " \
    "--extended-regexp --invert-grep --grep '^Release' #{directories.join(' ')}"

  output = `#{command}`.strip

  output.empty? ? 'No changes.' : output
end

changelog = File.expand_path('../CHANGELOG.md', __dir__)
options = {
  from: `git tag --sort taggerdate`.lines.last&.strip,
  to: 'HEAD',
  version: File.read(File.expand_path('../VERSION', __dir__)).strip
}

parser = OptionParser.new do |o|
  o.banner = 'Usage: changelog.rb [OPTIONS]'

  o.separator("\nOptions:\n")

  o.on('-h', '--help', 'Shows this help message') do
    abort o.to_s
  end

  o.on('-f', '--from SHA', 'The first commit or tag for the changelog') do |val|
    options[:from] = val unless val.empty?
  end

  o.on('-t', '--to SHA', 'The last commit or tag for the changelog') do |val|
    options[:to] = val unless val.empty?
  end

  o.on(
    '-v',
    '--version VERSION', 'The version to generate the changelog for'
  ) do |val|
    options[:version] = val unless val.empty?
  end
end

parser.parse!(ARGV)

from = options[:from]
to = options[:to]

commits = commits_in(
  [
    'Makefile',
    'compiler/Makefile',
    'runtime/Makefile',
    'vm/Makefile',
    'compiler/lib',
    'runtime/src',
    'vm/src'
  ],
  from,
  to
)

new_changelog = File.read(changelog).gsub('# Changelog', <<~CHANGELOG.strip)
  # Changelog

  ## #{options[:version]} - #{Time.now.strftime('%B %d, %Y')}

  #{commits}
CHANGELOG

File.open(changelog, 'w') do |handle|
  handle.write(new_changelog)
end
