#!/usr/bin/env ruby
# frozen_string_literal: true

new_version = ARGV[0]

if !new_version || new_version.empty?
  abort 'You must specify a new version as the first argument'
end

version_file = File.expand_path('../VERSION', __dir__)
compiler_version = File.expand_path('../compiler/lib/inkoc/version.rb', __dir__)
cargo_toml = File.expand_path('../vm/Cargo.toml', __dir__)

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

old_toml = File.read(cargo_toml)
new_toml =
  old_toml.gsub(/version.+# VERSION/, %(version = "#{new_version}" # VERSION))

File.open(cargo_toml, 'w') do |handle|
  handle.write(new_toml)
end
