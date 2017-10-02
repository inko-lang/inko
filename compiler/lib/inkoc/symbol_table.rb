# frozen_string_literal: true

module Inkoc
  class SymbolTable
    include Inspect
    include Enumerable

    attr_reader :mapping, :parent

    def initialize(parent = nil)
      @mapping = {}
      @parent = parent
    end

    def define(name, type, mutable = false)
      symbol = Symbol.new(name, type, @mapping.length, mutable)

      @mapping[name] = symbol

      symbol
    end

    def names
      @mapping.keys
    end

    def each
      return to_enum(__method__) unless block_given?

      @mapping.values.each do |value|
        yield value
      end
    end

    def [](name)
      @mapping[name] || NullSymbol.new(name)
    end

    def defined?(name)
      self[name].any?
    end

    def any?
      @mapping.any?
    end

    def empty?
      @mapping.empty?
    end

    def length
      @mapping.length
    end

    def ==(other)
      other.is_a?(self.class) &&
        mapping == other.mapping &&
        parent == other.parent
    end
  end
end
