# frozen_string_literal: true

module Inkoc
  class State
    attr_reader :config

    # Any diagnostics that were produced when compiling modules.
    attr_reader :diagnostics

    # The modules that have been compiled.
    attr_reader :modules

    # The database storing all type information.
    attr_reader :typedb

    # A cache containing relative module paths and their corresponding absolute
    # paths.
    attr_reader :module_paths_cache

    def initialize(config)
      @config = config
      @diagnostics = Diagnostics.new
      @modules = {}
      @typedb = TypeSystem::Database.new
      @module_paths_cache = ModulePathsCache.new(config)
    end

    def module_exists?(name)
      @modules.key?(name)
    end

    def module(name)
      @modules[name.to_s]
    end

    def store_module(mod)
      @modules[mod.name.to_s] = mod
    end

    def type_of_module_global(mod_name, global_name)
      self.module(mod_name)&.lookup_global(global_name)
    end

    def diagnostics?
      @diagnostics.any?
    end

    def errors?
      @diagnostics.errors?
    end

    def display_diagnostics(format = 'pretty')
      formatter =
      case format
      when 'pretty'
        Formatter::Pretty
      when 'json'
        Formatter::Json
      else
        raise ArgumentError, "The format #{format.inspect} is not supported"
      end

      output = formatter.new.format(@diagnostics)

      STDERR.puts(output) unless output.empty?
    end

    def find_module_path(path)
      @module_paths_cache.absolute_path_for(path)
    end

    def main_module_name_from_path(path)
      full_path, dir = find_module_path(path)

      return TIR::QualifiedName.new(%w[main]) unless full_path

      # This turns "runtime/std/env.inko" into "std::env".
      parts = full_path
        .to_s
        .gsub("#{dir.to_s.chomp(File::SEPARATOR)}#{File::SEPARATOR}", '')
        .gsub(Config::SOURCE_EXT, '')
        .split(File::SEPARATOR)

      TIR::QualifiedName.new(parts)
    end

    def inspect
      "Inkoc::State(modules: [#{modules.keys.join(', ')}])"
    end
  end
end
