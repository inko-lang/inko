# frozen_string_literal: true

module Inkoc
  class Diagnostic
    attr_reader :level, :message, :location

    def self.error(*args)
      new(:error, *args)
    end

    def self.warning(*args)
      new(:warning, *args)
    end

    def initialize(level, message, location)
      @level = level
      @message = message
      @location = location
    end

    def error?
      @level == :error
    end

    def line
      location.line
    end

    def column
      location.column
    end

    def file
      location.file
    end

    def path
      file.path
    end
  end
end
