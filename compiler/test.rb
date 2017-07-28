# frozen_string_literal: true

$LOAD_PATH.unshift(File.expand_path('../lib', __FILE__))

require 'inkoc'

compiler = Inkoc::Compiler.new(Inkoc::State.new(Inkoc::Config.new))
mod = compiler.compile('/tmp/test.inko')

if mod
  ins = mod.body.instructions

  ins.last(5).each_with_index do |i, index|
    puts "#{(index + 1).to_s.rjust(2, '0')}: #{i.inspect}"
    puts
  end
end

compiler.display_diagnostics
