# frozen_string_literal: true

module Inkoc
  class ConstantResolver
    attr_reader :diagnostics

    def initialize(diagnostics)
      @diagnostics = diagnostics
    end

    def resolve(node, scope)
      type = resolve_without_error(node, scope)

      if type.error?
        diagnostics.undefined_constant_error(node.qualified_name, node.location)
      end

      type
    end

    def resolve_without_error(node, scope)
      source =
        node.receiver ? resolve_without_error(node.receiver, scope) : scope

      source.lookup_type(node.name) || TypeSystem::Error.new
    end
  end
end
