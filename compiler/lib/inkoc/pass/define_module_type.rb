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
        @module.type = Inkoc::TypeSystem::Object
          .new(name: @module.name.to_s, prototype: @state.typedb.module_type)

        [ast]
      end
    end
  end
end
