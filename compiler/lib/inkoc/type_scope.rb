# frozen_string_literal: true

module Inkoc
  class TypeScope
    attr_reader :self_type, :block_type, :locals

    # self_type - The type of "self".
    # block_type - The type of the block that is being executed.
    # locals - A SymbolTable containing the local variables of the
    #          current scope.
    def initialize(self_type, block_type, locals)
      @self_type = self_type
      @block_type = block_type
      @locals = locals
    end

    def define_self_local
      locals.define(Config::SELF_LOCAL, self_type)
    end

    def type_of_local(name)
      _, local = locals.lookup_with_parent(name)

      local.type if local.any?
    end

    def closure?
      block_type.closure?
    end

    def method?
      block_type.method?
    end
  end
end
