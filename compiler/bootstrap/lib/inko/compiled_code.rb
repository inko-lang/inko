module Inko
  class CompiledCode
    attr_reader :name, :file, :line, :arguments, :required_arguments,
      :rest_argument, :integers, :floats, :strings, :code_objects,
      :register, :locals, :instructions, :register, :labels, :type

    attr_accessor :outer_scope

    def initialize(name, file, line, arguments = 0, required_arguments = 0,
                   rest_argument: false, type: nil)
      @name = name
      @file = file
      @line = line
      @arguments = arguments
      @required_arguments = required_arguments
      @rest_argument = rest_argument

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

    def closure?
      type == :closure
    end

    def local_defined?(name)
      @locals.include?(name) || closure? && outer_scope.local_defined?(name)
    end

    def resolve_local(name)
      if !@locals.include?(name) && closure?
        depth = 1
        current = outer_scope
        local = nil

        while current
          if current.locals.include?(name)
            local = current.locals.get(name)
            break
          end

          depth += 1
          current = current.outer_scope
        end

        unless local
          raise ArgumentError, "Local variable #{name.inspect} not found"
        end

        [depth, local]
      else
        [nil, @locals.get(name)]
      end
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
        @labels[pos] = @instructions.length
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

    def instruct(line, column)
      gen = InstructionGenerator.new(self, line, column)

      yield gen
    end

    Instruction::NAME_MAPPING.keys.each do |key|
      class_eval <<-EOF, __FILE__, __LINE__ + 1
        def #{key}(args, line, column)
          instruction(:#{key}, args, line, column)
        end
      EOF
    end
  end
end
