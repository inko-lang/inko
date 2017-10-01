# frozen_string_literal: true

module Inkoc
  class Symbol
    include Inspect

    attr_reader :name, :type, :index

    def initialize(name, type, index = -1, mutable = false)
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

    def ==(other)
      other.is_a?(Symbol) &&
        name == other.name &&
        type == other.type &&
        index == other.index &&
        mutable? == other.mutable?
    end
  end

  class NullSymbol
    include Inspect

    attr_reader :name, :type, :index

    def initialize(name)
      @name = name
      @type = Type::Dynamic.new
      @index = -1
    end

    def any?
      false
    end

    def mutable?
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
