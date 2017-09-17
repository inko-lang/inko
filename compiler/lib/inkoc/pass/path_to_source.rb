# frozen_string_literal: true

module Inkoc
  module Pass
    class PathToSource
      def initialize(mod, state)
        @module = mod
        @state = state
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
