#!/usr/bin/env ruby
# frozen_string_literal: true

load_dir = ARGV.fetch(0)
bin_file = ARGV.fetch(1)
data = File.read(bin_file)

File.open(bin_file, 'w') do |handle|
  handle.write(
    data.gsub('# LOAD_PATH_FOR_INSTALL', "$LOAD_PATH.unshift('#{load_dir}')")
  )
end
