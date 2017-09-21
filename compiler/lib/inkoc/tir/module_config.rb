# frozen_string_literal: true

module Inkoc
  module TIR
    class ModuleConfig
      VALID_KEYS = Set.new(
        %w[import_prelude import_bootstrap define_module]
      ).freeze

      def initialize
        @options = {
          import_prelude: true,
          import_bootstrap: true,
          define_module: true
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

      def define_module?
        @options[:define_module]
      end
    end
  end
end
