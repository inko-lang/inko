# frozen_string_literal: true

module Inkoc
  module Codegen
    class Literals
      include Inspect

      def initialize
        @values = {}
      end

      def add(value)
        @values[value] ||= @values.length
      end

      def get(value)
        @values[value]
      end

      def get_or_set(value)
        get(value) || add(value)
      end

      def to_a
        @values.keys
      end
    end
  end
end
