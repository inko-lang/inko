# frozen_string_literal: true

module Inkoc
  class TypeScope
    attr_reader :self_type, :block_type, :locals, :parent

    # self_type - The type of "self".
    # block_type - The type of the block that is being executed.
    # locals - A SymbolTable containing the local variables of the
    #          current scope.
    # parent - The parent scope, if any.
    def initialize(self_type, block_type, locals, parent = nil)
      @self_type = self_type
      @block_type = block_type
      @locals = locals
      @parent = parent
    end

    def define_self_local
      name = Config::SELF_LOCAL

      locals.define(name, self_type) if locals[name].nil?
    end

    def depth_and_symbol_for_local(name)
      depth, local = locals.lookup_with_parent(name)

      block_type.captures = true if depth >= 0

      [depth, local] if local.any?
    end

    def closure?
      block_type.closure?
    end

    def method?
      block_type.method?
    end

    def method_block_type
      current = self
      current = current.parent while current && !current.method?

      current&.block_type
    end
  end
end
