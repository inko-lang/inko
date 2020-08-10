# frozen_string_literal: true

module Inkoc
  module Pass
    class TrackModule
      def initialize(compiler, mod)
        @module = mod
        @state = compiler.state
      end

      def run(ast)
        @state.store_module(@module)

        [ast]
      end
    end
  end
end
