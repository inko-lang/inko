# frozen_string_literal: true

module Inkoc
  module Pass
    class DeadCode
      def initialize(mod, state)
        @module = mod
        @state = state
      end

      def run
        on_code_object(@module.body)

        []
      end

      def on_code_object(code_object)
        code_object.blocks.each do |block|
          next if code_object.reachable_basic_block?(block) || block.empty?

          diagnostics.unreachable_code_warning(block.location)
        end

        code_object.code_objects.each { |code| on_code_object(code) }
      end

      def diagnostics
        @state.diagnostics
      end
    end
  end
end
