module Aeon
  class Compiler
    CLASS_PROTOTYPE = '__prototype'

    attr_reader :source_file

    def initialize(source_file)
      @source_file = source_file
    end

    def compile
      content = File.read(@source_file)
      ast     = Parser.new(content).parse

      code = CompiledCode.new('main', @source_file, 1)

      process(ast, code)

      code.resolve_labels

      code
    end

    def process(node, *args)
      send(:"on_#{node.type}", node, *args)
    end

    def on_exprs(node, current_cc)
      node.children.each do |child|
        process(child, current_cc)
      end

      last_ins = current_cc.instructions.last

      last_ins.arguments[0] if last_ins
    end

    def on_compile_flag(node, current_cc)
      # TODO: implement compiler flags
    end

    def on_def(node, current_cc)
      Compilation::Method.new(self, node, current_cc).compile
    end

    def on_class(node, current_cc)
      Compilation::Class.new(self, node, current_cc).compile
    end

    def on_send(node, current_cc)
      name = node.children[1]

      if name.start_with?('__') and Instruction::NAME_MAPPING.key?(name[2..-1].to_sym)
        compiler = Compilation::SendInstruction.new(self, node, current_cc)
      else
        compiler = Compilation::Send.new(self, node, current_cc)
      end

      compiler.compile
    end

    def on_return(node, current_cc)
      Compilation::Return.new(self, node, current_cc).compile
    end

    def on_let(node, current_cc)
      Compilation::Let.new(self, node, current_cc).compile
    end

    def on_self(node, current_cc)
      Compilation::Self.new(node, current_cc).compile
    end

    def on_ident(node, current_cc)
      Compilation::Identifier.new(self, node, current_cc).compile
    end

    def on_ivar(node, current_cc)
      Compilation::InstanceVariable.new(node, current_cc).compile
    end

    def on_const(node, current_cc)
      Compilation::Constant.new(self, node, current_cc).compile
    end

    def on_sstring(node, current_cc)
      Compilation::String.new(self, node, current_cc).compile
    end

    def on_dstring(node, current_cc)
      Compilation::String.new(self, node, current_cc, true).compile
    end

    def on_integer(node, current_cc)
      Compilation::Integer.new(node, current_cc).compile
    end

    def on_float(node, current_cc)
      Compilation::Float.new(node, current_cc).compile
    end

    def on_true(node, current_cc)
      Compilation::True.new(node, current_cc).compile
    end

    def on_false(node, current_cc)
      Compilation::False.new(node, current_cc).compile
    end
  end
end
