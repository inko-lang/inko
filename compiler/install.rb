#!/usr/bin/env ruby
# frozen_string_literal: true

require 'fileutils'

target_lib_dir = ENV['LIBDIR']
target_bin_dir = ENV['BINDIR']
src_lib_dir = File.expand_path('lib', __dir__)
compiler_dir = File.join(target_lib_dir, 'compiler')

if !target_lib_dir || target_lib_dir.empty?
  abort 'The LIBDIR environment variable must be specified'
end

if !target_bin_dir || target_bin_dir.empty?
  abort 'The BINDIR environment variable must be specified'
end

FileUtils.mkdir_p([target_lib_dir, target_bin_dir])
FileUtils.cp_r(src_lib_dir, compiler_dir)

Dir['./bin/*'].each do |local_bin|
  data = File.read(local_bin)
  target_bin = File.join(target_bin_dir, File.basename(local_bin))

  File.open(target_bin, 'w') do |handle|
    handle.write(
      data.gsub(
        '# LOAD_PATH_FOR_INSTALL',
        "$LOAD_PATH.unshift('#{compiler_dir}')"
      )
    )
  end

  FileUtils.chmod(0o755, target_bin)
end
