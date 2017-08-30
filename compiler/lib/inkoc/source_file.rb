# frozen_string_literal: true

module Inkoc
  class SourceFile
    attr_reader :path

    def initialize(path)
      @path = path
    end

    def lines
      @lines ||= File.file?(path) ? File.readlines(path) : []
    end
  end
end
