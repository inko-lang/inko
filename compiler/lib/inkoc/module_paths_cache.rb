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

        # If the path is outside one of the load directories, it either can't be
        # loaded or is the main module.
        next unless full_path.to_s.start_with?(dir.to_s)

        if full_path.file?
          return @cache[path] = [full_path, dir]
        end
      end

      nil
    end
  end
end
