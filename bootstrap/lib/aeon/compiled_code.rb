module Aeon
  class CompiledCode
    attr_reader :name, :file, :line, :required_arguments, :visibility,
      :integers, :floats, :strings, :code_objects, :register, :locals,
      :instructions, :register, :labels, :type

    def initialize(name, file, line, required_arguments = 0, visibility = :private, type = nil)
      @name = name
      @file = file
      @line = line
      @required_arguments = required_arguments
      @visibility = visibility

      @locals = Literals.new
      @instructions = []
      @integers = Literals.new
      @floats = Literals.new
      @strings = Literals.new
      @code_objects = Literals.new

      @label = 0
      @labels = {}

      @register = -1

      @type = type
    end

    def next_register
      @register += 1
    end

    def instruction(*args)
      @instructions << Instruction.new(*args)

      self
    end

    def label
      @label -= 1
    end

    def mark_label(pos)
      if @instructions.empty?
        raise ArgumentError, "Can't mark label when there are no instructions"
      else
        @labels[pos] = @instructions.length - 1
      end
    end

    def resolve_labels
      @instructions.each do |ins|
        ins.remap_arguments do |arg|
          arg < 0 ? @labels[arg] : arg
        end
      end

      @code_objects.to_a.each(&:resolve_labels)
    end

    Instruction::NAME_MAPPING.keys.each do |key|
      class_eval <<-EOF, __FILE__, __LINE__ + 1
        def ins_#{key}(args, line, column)
          instruction(:#{key}, args, line, column)
        end
      EOF
    end
  end
end
