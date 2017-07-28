# frozen_string_literal: true

module Inkoc
  module TIR
    class CodeObject
      include Inspect

      attr_reader :locals, :instructions, :registers, :location

      def initialize(location)
        @locals = SymbolTable.new
        @instructions = []
        @registers = VirtualRegisters.new
        @location = location
      end

      def define_self_local(type)
        @locals.define(Config::SELF_LOCAL, type, false)
      end

      def register(type)
        @registers.allocate(type)
      end

      def instruct(klass, *args)
        @instructions << Instruction.const_get(klass).new(*args)
      end
    end
  end
end
