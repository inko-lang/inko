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
        full_path = dir.join(path).expand_path

        return @cache[path] = full_path if full_path.file?
      end

      nil
    end
  end
end
