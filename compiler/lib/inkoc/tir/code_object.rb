# frozen_string_literal: true

module Inkoc
  module TIR
    class CodeObject
      include Inspect

      attr_reader :name, :type, :locals, :registers, :location, :blocks,
                  :code_objects

      def initialize(name, type, location)
        @name = name
        @type = type
        @locals = SymbolTable.new
        @registers = VirtualRegisters.new
        @location = location
        @blocks = []
        @code_objects = []
      end

      def arguments_count
        @type.arguments_count
      end

      def required_arguments_count
        @type.required_arguments_count
      end

      def rest_argument?
        @type.rest_argument
      end

      def local_variables_count
        @locals.length
      end

      def registers_count
        @registers.length
      end

      def start_block
        @blocks.first
      end

      def current_block
        @blocks.last
      end

      def each_reachable_basic_block
        block = start_block

        while block
          yield block

          block = block.next
        end
      end

      def reachable_basic_block?(block)
        block == start_block || block.previous
      end

      def define_local(name, type, mutable)
        @locals.define(name, type, mutable)
      end

      def define_immutable_local(name, type)
        define_local(name, type, false)
      end

      def define_self_local(type)
        define_immutable_local(Config::SELF_LOCAL, type)
      end

      def register(type)
        @registers.allocate(type)
      end

      def register_dynamic
        register(Type::Dynamic.new)
      end

      def instruct(*args)
        current_block.instruct(*args)
      end

      def set_integer(value, type, location)
        set_literal(:SetInteger, value, type, location)
      end

      def set_float(value, type, location)
        set_literal(:SetFloat, value, type, location)
      end

      def set_array(values, type, location)
        reg = register(type)

        instruct(:SetArray, reg, values, location)

        reg
      end

      def set_hash_map(keys, values, type, location)
        set_literal(:SetHashMap, keys.zip(values), type, location)
      end

      def set_local(symbol, value, location)
        instruct(:SetLocal, symbol, value, location)

        value
      end

      def local_exists(bool_type, local, location)
        reg = register(bool_type)

        instruct(:LocalExists, reg, local, location)

        reg
      end

      def goto_next_block_if_true(register, location)
        instruct(:GotoNextBlockIfTrue, register, location)

        register
      end

      def get_local(symbol, location)
        reg = register(symbol.type)

        instruct(:GetLocal, reg, symbol, location)

        reg
      end

      def get_global(symbol, location)
        reg = register(symbol.type)

        instruct(:GetGlobal, reg, symbol, location)

        reg
      end

      def self_local
        locals[Config::SELF_LOCAL]
      end

      def self_type
        self_local.type
      end

      def send_object_message(register, receiver, name, arguments, location)
        instruct(
          :SendObjectMessage,
          register,
          receiver,
          name,
          arguments,
          location
        )

        register
      end

      def return_value(value, location)
        instruct(:Return, value, location)

        value
      end

      def get_toplevel(type, location)
        reg = register(type)

        instruct(:GetToplevel, reg, location)

        reg
      end

      def get_nil(type, location)
        reg = register(type)

        instruct(:GetNil, reg, location)

        reg
      end

      def set_block(block, type, location)
        reg = register(type)

        instruct(:SetBlock, reg, block, location)

        reg
      end

      def load_module(path, location)
        reg = register_dynamic

        instruct(:LoadModule, reg, path, location)

        reg
      end

      def get_attribute(receiver, name, type, location)
        reg = register(type)

        instruct(:GetAttribute, reg, receiver, name, location)

        reg
      end

      def set_attribute(receiver, name, value, location)
        reg = register(value.type)

        instruct(:SetAttribute, reg, receiver, name, value, location)

        reg
      end

      def get_true(type, location)
        reg = register(type)

        instruct(:GetTrue, reg, location)

        reg
      end

      def set_object(type, permanent, prototype, location)
        reg = register(type)

        instruct(:SetObject, reg, permanent, prototype, location)

        reg
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

      def visitor_method
        :on_code_object
      end
    end
  end
end
