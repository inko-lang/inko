# frozen_string_literal: true

module Inkoc
  class ConstantResolver
    attr_reader :diagnostics

    def initialize(diagnostics)
      @diagnostics = diagnostics
    end

    def resolve(node, scope)
      type = scope.lookup_type(node.name) || TypeSystem::Error.new

      if type.error?
        diagnostics.undefined_constant_error(node.name, node.location)
      end

      type
    end
  end
end
