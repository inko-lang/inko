# frozen_string_literal: true

module Inkoc
  class SymbolTable
    include Inspect

    def initialize
      @map = {}
    end

    def define(name, type, mutable = false)
      symbol = Symbol.new(name, type, @map.length, mutable)

      @map[name] = symbol

      symbol
    end

    def [](name)
      @map[name] || NullSymbol.new(name)
    end

    def defined?(name)
      self[name].any?
    end

    def empty?
      @map.empty?
    end
  end
end
