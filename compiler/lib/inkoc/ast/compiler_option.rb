# frozen_string_literal: true

module Inkoc
  module AST
    class CompilerOption
      include Predicates
      include Inspect

      attr_reader :key, :value, :location

      def initialize(key, value, location)
        @key = key
        @location = location

        @value =
          case value
          when 'true'
            true
          when 'false'
            false
          else
            value
          end
      end

      def visitor_method
        :on_compiler_option
      end
    end
  end
end
