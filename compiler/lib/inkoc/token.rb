# frozen_string_literal: true

module Inkoc
  class Token
    include Inspect

    attr_accessor :type, :value
    attr_reader :location

    # type - The type of the token as a Symbol.
    # value - The value of the token as a String.
    # location - The source location of the token.
    def initialize(type, value, location)
      @type = type
      @value = value
      @location = location
    end

    def valid?
      true
    end

    def line
      @location.line
    end

    def column
      @location.column
    end

    def valid_but_not?(type)
      valid? && @type != type
    end
  end

  class NullToken < Token
    attr_reader :type, :value, :location

    def initialize
      @type = :'<end-of-input>'
      @value = '<end-of-input>'
      @location = nil
    end

    def nil?
      true
    end

    def valid?
      false
    end

    def type=(*)
      # noop since null tokens don't have a type
    end

    def line
      1
    end

    def column
      1
    end

    def inspect
      'NullToken()'
    end
  end
end
