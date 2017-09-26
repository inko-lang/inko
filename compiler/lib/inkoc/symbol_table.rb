# frozen_string_literal: true

module Inkoc
  class SymbolTable
    include Inspect
    include Enumerable

    def initialize
      @map = {}
    end

    def define(name, type, mutable = false)
      symbol = Symbol.new(name, type, @map.length, mutable)

      @map[name] = symbol

      symbol
    end

    def names
      @map.keys
    end

    def each
      @map.values.each do |value|
        yield value
      end
    end

    def [](name)
      @map[name] || NullSymbol.new(name)
    end

    def defined?(name)
      self[name].any?
    end

    def any?
      @map.any?
    end

    def empty?
      @map.empty?
    end

    def length
      @map.length
    end
  end
end