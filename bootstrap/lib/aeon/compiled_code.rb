module Aeon
  class CompiledCode
    attr_reader :name, :file, :line, :required_arguments, :method_visibility,
      :integers, :floats, :strings, :code_objects, :register, :locals,
      :instructions

    def initialize(name, file, line, required_arguments = 0, method_visibility = :private)
      @name = name
      @file = file
      @line = line
      @required_arguments = required_arguments
      @method_visibility = method_visibility

      @locals = []
      @instructions = []
      @integers = Literals.new
      @floats = Literals.new
      @strings = Literals.new
      @code_objects = Literals.new

      @register = -1
    end

    def next_register
      @register += 1
    end

    def add_local(name)
      @locals << name
    end

    def add_instruction(*args)
      @instructions << Instruction.new(*args)

      self
    end
  end
end
