# frozen_string_literal: true

module Inkoc
  module Pass
    class RefineModuleType
      def initialize(mod, state)
        @module = mod
        @state = state
      end

      def typedb
        @state.typedb
      end

      def run(ast)
        top = typedb.top_level
        modules = top.lookup_attribute(Config::MODULES_ATTRIBUTE).type
        proto = top.lookup_attribute(Config::MODULE_TYPE).type

        @module.type.prototype = proto

        modules.define_attribute(@module.type.name, @module.type)

        [ast]
      end
    end
  end
end
