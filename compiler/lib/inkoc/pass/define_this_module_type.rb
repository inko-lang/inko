# frozen_string_literal: true

module Inkoc
  module Pass
    class DefineThisModuleType
      include VisitorMethods

      def initialize(mod, *)
        @module = mod
      end

      def run(ast)
        @module.globals.define(Config::MODULE_GLOBAL, @module.type)

        [ast]
      end
    end
  end
end
