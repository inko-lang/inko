# frozen_string_literal: true

module Inkoc
  class Symbol
    include Inspect

    attr_reader :name, :type, :index

    def initialize(name, type, index, mutable)
      @name = name
      @type = type
      @index = index
      @mutable = mutable
    end

    def any?
      true
    end

    def mutable?
      @mutable
    end

    def or_else
      self
    end
  end

  class NullSymbol < Symbol
    def initialize(name)
      super(name, Type::Dynamic.new, -1, false)
    end

    def any?
      false
    end

    def nil?
      true
    end

    def or_else
      yield
    end
  end
end
