# frozen_string_literal: true

module Inkoc
  module Codegen
    class CompiledCode
      include Inspect

      attr_reader :name, :instructions, :code_objects

      attr_accessor :arguments, :required_arguments, :locals,
                    :registers, :captures, :catch_table, :file, :line

      def initialize(name, file, line)
        @name = name
        @file = file
        @line = line
        @arguments = []
        @rest_argument = false
        @locals = 0
        @registers = 0
        @captures = false
        @instructions = []
        @code_objects = Literals.new
        @catch_table = []
      end

      def instruct(*args)
        @instructions << Instruction.named(*args)
      end
    end
  end
end
