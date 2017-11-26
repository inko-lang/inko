# frozen_string_literal: true

module Inkoc
  module Pass
    class OptimiseArray
      include VisitorMethods

      def initialize(mod, state)
        @module = mod
        @state = state
      end

      def run
        process_node(@module.body)

        []
      end

      def on_code_object(code_object)
        code_object.each_reachable_basic_block do |basic_block|
          basic_block.instructions.each do |tir_ins|
            #process_node(tir_ins, compiled_code)
          end
        end
      end
    end
  end
end
