# frozen_string_literal: true

module Inkoc
  module Codegen
    class CompiledCode
      include Inspect

      attr_reader :name, :instructions, :literals, :code_objects

      attr_accessor :arguments, :required_arguments, :locals,
                    :registers, :captures, :catch_table

      def initialize(name, location)
        @name = name
        @location = location
        @arguments = []
        @rest_argument = false
        @locals = 0
        @registers = 0
        @captures = false
        @instructions = []
        @literals = Literals.new
        @code_objects = Literals.new
        @catch_table = []
      end

      def file
        @location.file.path
      end

      def line
        @location.line
      end

      def instruct(*args)
        @instructions << Instruction.named(*args)
      end
    end
  end
end
