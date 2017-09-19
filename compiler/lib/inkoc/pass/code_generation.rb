# frozen_string_literal: true

module Inkoc
  module Pass
    class CodeGeneration
      include VisitorMethods

      def initialize(mod, state)
        @module = mod
        @state = state
      end

      def run
        compiled_code = on_module_body

        [compiled_code]
      end

      def on_module_body
        compiled_code =
          Codegen::CompiledCode.new(@module.name.to_s, @module.location)

        process_node(@module.body, compiled_code)

        compiled_code
      end

      def on_code_object(code_object, compiled_code)
        code_object.each_reachable_basic_block do |basic_block|
          basic_block.instructions.each do |tir_ins|
            process_node(tir_ins, compiled_code)
          end
        end
      end

      def on_get_array_prototype(tir_ins, compiled_code)
        compiled_code.get_array_prototype(tir_ins, tir_ins.location)
      end

      def on_set_literal(tir_ins, compiled_code)
        compiled_code
          .set_literal(tir_ins.register, tir_ins.value, tir_ins.location)
      end

      def on_return(tir_ins, compiled_code)
        compiled_code.return(tir_ins.register, tir_ins.location)
      end
    end
  end
end
