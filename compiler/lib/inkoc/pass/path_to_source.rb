# frozen_string_literal: true

module Inkoc
  module Pass
    class PathToSource
      def initialize(compiler, mod)
        @module = mod
        @state = compiler.state
      end

      def run
        [@module.source_code]
      rescue SystemCallError => error
        @state.diagnostics.error(error.message, @module.location)
        nil
      end
    end
  end
end
