module Inko
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
      unless found = @values[value]
        raise ArgumentError, "Undefined literal #{value.inspect}"
      end

      found
    end

    def get_or_set(value)
      include?(value) ? get(value) : add(value)
    end

    def to_a
      @values.keys
    end

    def inspect
      "Literals(#{to_a.map(&:inspect).join(', ')})"
    end
  end
end
