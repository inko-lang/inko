# frozen_string_literal: true

module Inkoc
  module Pass
    class DefineModuleType
      def initialize(mod, state)
        @module = mod
        @state = state
      end

      def typedb
        @state.typedb
      end

      def run(ast)
        @module.type =
          if @module.define_module?
            define_module_type
          else
            typedb.top_level
          end

        [ast]
      end

      def define_module_type
        Inkoc::TypeSystem::Object.new(name: @module.name.to_s)
      end
    end
  end
end
