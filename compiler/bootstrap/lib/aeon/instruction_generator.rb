module Aeon
  class InstructionGenerator
    def initialize(code, line, column)
      @code = code
      @line = line
      @column = column
    end

    Instruction::NAME_MAPPING.keys.each do |key|
      class_eval <<-EOF, __FILE__, __LINE__ + 1
        def #{key}(*args)
          @code.instruction(:#{key}, args, @line, @column)
        end
      EOF
    end
  end
end
