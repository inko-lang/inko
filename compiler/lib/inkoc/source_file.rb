# frozen_string_literal: true

module Inkoc
  class SourceFile
    attr_reader :path

    def initialize(path)
      @path = path.is_a?(Pathname) ? path : Pathname.new(path)
    end

    def lines
      @lines ||= path.file? ? path.readlines : []
    end
  end
end
