# frozen_string_literal: true

module Inkoc
  class State
    attr_reader :config

    # Any diagnostics that were produced when compiling modules.
    attr_reader :diagnostics

    # All the compiled modules, mapped to their names. The values of this hash
    # are explicitly set to nil when:
    #
    # * The module was found and is about to be processed for the first time
    # * The module could not be found
    #
    # This prevents recursive imports from causing the compiler to get stuck in
    # a loop.
    attr_reader :modules

    # The database storing all type information.
    attr_reader :typedb

    def initialize(config)
      @config = config
      @diagnostics = Diagnostics.new
      @modules = {}
      @typedb = Type::Database.new
    end

    def module_compiled?(name)
      @modules.key?(name)
    end

    def track_module_before_compilation(name)
      @modules[name] = nil unless @modules[name]
    end

    def store_module(name, mod)
      @modules[name] = mod
    end
  end
end
