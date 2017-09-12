# frozen_string_literal: true

module Inkoc
  class ModulePathsCache
    def initialize(config)
      @config = config
      @cache = {}
    end

    def absolute_path_for(path)
      if (cached = @cache[path])
        return cached
      end

      @config.source_directories.each do |dir|
        full_path = File.expand_path(File.join(dir, path))

        return @cache[path] = full_path if File.file?(full_path)
      end

      nil
    end
  end
end
