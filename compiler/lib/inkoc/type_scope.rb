# frozen_string_literal: true

module Inkoc
  class TypeScope
    attr_reader :self_type, :block_type, :module, :locals, :parent

    def initialize(self_type, block_type, mod, locals = nil, parent = nil)
      @self_type = self_type
      @block_type = block_type
      @module = mod
      @locals = locals
      @parent = parent
    end

    def module_type
      @module.type
    end

    def define_self_argument
      symbol = block_type.define_self_argument(self_type)

      locals&.add_symbol(symbol)
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

    def module_scope?
      self_type == module_type
    end

    def constructor?
      if (method = method_block_type)
        self_type.object? && method.name == Inkoc::Config::INIT_MESSAGE
      else
        false
      end
    end

    def method_block_type
      current = self
      current = current.parent while current && !current.method?

      current&.block_type
    end

    def lookup_constant(name)
      block_type.lookup_type(name) ||
        self_type.lookup_type(name) ||
        @module.lookup_type(name)
    end
    alias lookup_type lookup_constant

    def lookup_method(name)
      self_type.lookup_method(name)
        .or_else { module_type.lookup_method(name) }
    end
  end
end
