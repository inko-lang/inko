# frozen_string_literal: true

module Inkoc
  # Compiler for translating Inko source files into IVM bytecode.
  class Compiler
    def initialize(config)
      @state = State.new(config)
    end

    # path - The file path of the module to compile, as a String.
    def compile(path)
      TIR::Builder.new(@state).build_main(path)
    end

    def diagnostics?
      @state.diagnostics.any?
    end

    def display_diagnostics
      formatter = Formatter::Pretty.new
      output = formatter.format(@state.diagnostics)

      STDERR.puts(output)
    end
  end
end
