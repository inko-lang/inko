module Aeon
  class Literals
    def initialize
      @values = {}
    end

    def add(value)
      @values[value] ||= @values.length
    end

    def include?(value)
      @values.key?(value)
    end

    def get(value)
      @values.fetch(value)
    end

    def get_or_set(value)
      include?(value) ? get(value) : add(value)
    end

    def to_a
      @values.keys
    end
  end
end
