# frozen_string_literal: true

module Inkoc
  module TIR
    class ModuleConfig
      DEFAULTS = {
        import_prelude: true,
        import_bootstrap: true,
        import_globals: true,
        define_module: true,
        import_trait_module: false,
      }.freeze

      def initialize
        @options = DEFAULTS.dup
      end

      def valid_key?(key)
        DEFAULTS.key?(key.to_sym)
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

      def import_globals?
        @options[:import_globals]
      end

      def define_module?
        @options[:define_module]
      end

      def import_trait_module?
        @options[:import_trait_module]
      end
    end
  end
end
