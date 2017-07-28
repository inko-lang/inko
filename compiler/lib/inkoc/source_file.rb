# frozen_string_literal: true

module Inkoc
  class SourceFile
    attr_reader :path

    def initialize(path)
      @path = path
    end
  end
end
