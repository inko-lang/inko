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
        top = typedb.top_level
        modules = top.lookup_attribute(Config::MODULES_ATTRIBUTE).type
        proto = top.lookup_attribute(Config::MODULE_TYPE).type
        type = Inkoc::TypeSystem::Object
          .new(name: @module.name.to_s, prototype: proto)

        modules.define_attribute(type.name, type)

        type
      end
    end
  end
end
