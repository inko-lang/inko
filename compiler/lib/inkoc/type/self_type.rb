# frozen_string_literal: true

module Inkoc
  module Type
    class SelfType
      include Inspect
      include Predicates

      def type_name
        'Self'
      end

      def self_type?
        true
      end

      def resolve_type(self_type, *)
        self_type
      end

      def type_compatible?(other)
        other.dynamic? || other.self_type?
      end

      def strict_type_compatible?(other)
        other.self_type?
      end

      def initialize_as(type, *)
        type
      end
    end
  end
end
