# frozen_string_literal: true

module Inkoc
  module Pass
    class TrackModule
      def initialize(mod, state)
        @module = mod
        @state = state
      end

      def run(ast)
        @state.store_module(@module)

        [ast]
      end
    end
  end
end
