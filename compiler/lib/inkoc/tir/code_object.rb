# frozen_string_literal: true

module Inkoc
  module TIR
    class CodeObject
      include Inspect

      attr_reader :name, :locals, :registers, :location, :blocks, :code_objects

      def initialize(name, location)
        @name = name
        @locals = SymbolTable.new
        @registers = VirtualRegisters.new
        @location = location
        @blocks = []
        @code_objects = []
      end

      def start_block
        @blocks.first
      end

      def current_block
        @blocks.last
      end

      def reachable_basic_block?(block)
        block == start_block || block.callers.any?
      end

      def define_self_local(type)
        @locals.define(Config::SELF_LOCAL, type, false)
      end

      def register(type)
        @registers.allocate(type)
      end

      def instruct(*args)
        current_block.instruct(*args)
      end

      def add_code_object(*args)
        object = CodeObject.new(*args)
        @code_objects << object

        object
      end

      def add_basic_block(*args)
        push_basic_block(new_basic_block(*args))
      end

      def add_connected_basic_block(*args)
        block = new_basic_block(*args)
        current_block&.next = block

        push_basic_block(block)
      end

      def push_basic_block(block)
        @blocks << block

        block
      end

      def new_basic_block(name = @blocks.length.to_s, *args)
        BasicBlock.new(name, *args)
      end
    end
  end
end
