# frozen_string_literal: true

module Inkoc
  class TypeScope
    attr_reader :self_type, :block_type, :module, :locals, :parent,
                :remapped_local_types

    def initialize(
      self_type,
      block_type,
      mod,
      locals: nil,
      parent: nil,
      enclosing_method: parent&.enclosing_method
    )
      @self_type = self_type
      @block_type = block_type
      @module = mod
      @locals = locals
      @parent = parent
      @enclosing_method = enclosing_method
    end

    def define_receiver_type
      block_type.self_type = self_type
    end

    def enclosing_method
      if @enclosing_method
        @enclosing_method
      elsif block_type.method?
        block_type
      end
    end

    def module_type
      @module.type
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
      self_type.base_type == module_type
    end

    def constructor?
      if (method = enclosing_method)
        self_type.object? && method.name == Inkoc::Config::INIT_MESSAGE
      else
        false
      end
    end

    def lookup_constant(name)
      block_type.lookup_type(name) ||
        enclosing_method&.lookup_type(name) ||
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
