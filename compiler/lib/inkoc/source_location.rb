# frozen_string_literal: true

module Inkoc
  class SourceLocation
    attr_reader :line, :column, :file

    def self.first_line(file)
      new(1, 1, file)
    end

    def initialize(line, column, file)
      @line = line
      @column = column
      @file = file
    end
  end
end
