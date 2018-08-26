# frozen_string_literal: true

module Inkoc
  module TypePass
    def initialize(mod, state)
      @module = mod
      @state = state
    end

    def diagnostics
      @state.diagnostics
    end

    def typedb
      @state.typedb
    end

    def run(ast)
      locals = ast.locals
      scope = TypeScope
        .new(@module.type, @module.body.type, @module, locals: locals)

      on_module_body(ast, scope)

      [ast]
    end

    def on_module_body(_node, _scope)
      raise NotImplementedError
    end

    def define_type(node, scope, *extra)
      type = process_node(node, scope, *extra)

      node.type ||= type if type
    end

    def define_types(nodes, scope, *extra)
      nodes.map { |n| define_type(n, scope, *extra) }
    end
  end
end
