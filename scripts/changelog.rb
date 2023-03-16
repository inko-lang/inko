#!/usr/bin/env ruby
# frozen_string_literal: true

require 'time'
require 'yaml'

placeholder = '<!-- new section -->'
categories = YAML.load_file('changelog.yml')
path = 'CHANGELOG.md'
data = File.read(path)
version = ARGV[0] || abort('You must specify a version')
start =
  if ARGV[1]
    ARGV[1]
  else
    `git tag --sort='-version:refname' | head -n1`.strip
  end
stop = ARGV[2] || 'HEAD'

output = `git log \
  --format="%h\t%(trailers:key=Changelog,valueonly=true,separator=%x2C)\t%s" \
  --no-merges \
  --grep='Changelog:' #{start}...#{stop}`.strip

grouped = Hash.new { |h, k| h[k] = [] }

output.each_line do |line|
  commit, category, summary = line.split("\t")

  next if category.empty?

  grouped[category] << [commit, summary.strip]
end

lines = [
  "#{placeholder}\n",
  "## #{version} (#{Time.now.utc.strftime('%Y-%m-%d')})"
]

if grouped.empty?
  lines.push("\nNo changes")
else
  categories.each do |key, label|
    entries = grouped[key]

    next if entries.empty?

    lines.push("\n### #{label}\n")

    entries.each do |(sha, summary)|
      lines.push("* #{sha}: #{summary}")
    end
  end
end

File.open(path, 'w') do |file|
  file.write(data.gsub(placeholder, lines.join("\n")))
end
