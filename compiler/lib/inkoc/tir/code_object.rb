# frozen_string_literal: true

module Inkoc
  module TIR
    class CodeObject
      include Inspect

      attr_reader :name, :type, :locals, :registers, :location, :blocks,
                  :code_objects, :catch_table

      def initialize(name, type, location, locals: SymbolTable.new)
        @name = name
        @type = type
        @locals = locals
        @registers = VirtualRegisters.new
        @location = location
        @blocks = []
        @code_objects = []
        @catch_table = CatchTable.new
      end

      def captures?
        type.closure?
      end

      def arguments_count
        @type.arguments_count
      end

      def argument_names
        @type.arguments.names
      end

      def required_arguments_count
        @type.required_arguments
      end

      def rest_argument?
        @type.last_argument_is_rest
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

      def last_instruction
        block = current_block
        block = block.previous while block.empty? && block.previous

        block.instructions.last
      end

      def each_reachable_basic_block
        return to_enum(__method__) unless block_given?

        block = start_block

        while block
          yield block

          block = block.next
        end
      end

      def reachable_basic_block?(block)
        catch_table.jump_to?(block) ||
          block.empty? ||
          block == start_block ||
          current_block == block ||
          block.previous
      end

      def define_local(name, type, mutable)
        @locals.define(name, type, mutable)
      end

      def define_immutable_local(name, type)
        define_local(name, type, false)
      end

      def register(type)
        @registers.allocate(type)
      end

      def register_dynamic
        register(TypeSystem::Dynamic.new)
      end

      def instruct(*args)
        instruction = current_block.instruct(*args)
        instruction.register
      end

      def self_local
        locals[Config::SELF_LOCAL]
      end

      def self_type
        self_local.type
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

      def return_type
        current_block.last_instruction.register.type
      end

      def visitor_method
        :on_code_object
      end
    end
  end
end
