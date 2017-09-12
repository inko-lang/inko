# frozen_string_literal: true

module Inkoc
  module TIR
    class ModuleConfig
      VALID_KEYS = Set.new(
        %w[import_prelude import_bootstrap]
      ).freeze

      def initialize
        @options = {
          import_prelude: true,
          import_bootstrap: true
        }
      end

      def valid_key?(key)
        VALID_KEYS.include?(key)
      end

      def []=(key, value)
        @options[key.to_sym] = value
      end

      def import_prelude?
        @options[:import_prelude]
      end

      def import_bootstrap?
        @options[:import_bootstrap]
      end
    end
  end
end
