#!/usr/bin/env ruby
# rubocop: disable all
# frozen_string_literal: true

require 'English'

new_version = ARGV[0]

if !new_version || new_version.empty?
  abort 'You must specify a new version as the first argument'
end

version_file = File.expand_path('../VERSION', __dir__)
compiler_version = File.expand_path('../compiler/lib/inkoc/version.rb', __dir__)
cargo_files = Dir['./{vm,cli}/Cargo.toml']

File.open(version_file, 'w') do |handle|
  handle.puts(new_version)
end

File.open(compiler_version, 'w') do |handle|
  handle.write(<<~RUBY)
    # frozen_string_literal: true

    module Inkoc
      VERSION = '#{new_version}'
    end
  RUBY
end

cargo_files.each do |cargo_toml|
  old_toml = File.read(cargo_toml)
  new_toml = old_toml.gsub(
    /version.+# VERSION/,
    %(version = "#{new_version}" # VERSION)
  )

  File.open(cargo_toml, 'w') do |handle|
    handle.write(new_toml)
  end
end

# Make sure that Cargo.lock is also updated
output = `make vm/check 2>&1`

unless $CHILD_STATUS.success?
  abort "Failed to update Cargo.lock files: #{output}"
end
