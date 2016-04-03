module Aeon
  class Compiler
    def initialize(source_file)
      @source_file = source_file
    end

    def compile
      content = File.read(@source_file)
      ast     = Parser.new(content).parse

      code = CompiledCode.new('main', @source_file, 1)

      process(ast, code)

      code
    end

    def process(node, *args)
      send(:"on_#{node.type}", node, *args)
    end

    def on_exprs(node, current_cc)
      node.children.each do |child|
        process(child, current_cc)
      end
    end

    def on_def(node, current_cc)
      name, vis, _, args, _, body = *node

      unless current_cc.strings.include?(name)
        current_cc.strings.add(name)
      end

      req_args = args.children.count do |arg|
        arg.children[2].nil?
      end

      method = CompiledCode.new(name, @source_file, node.line, req_args, vis)

      args.children.each do |arg|
        method.add_local(arg.children[0])
      end

      process(body, method)

      cc_idx   = current_cc.code_objects.add(method)
      self_idx = current_cc.next_register
      name_idx = current_cc.strings.get(name)

      line = node.line
      col  = node.column

      current_cc
        .add_instruction(:get_self, [self_idx], line, col)
        .add_instruction(:def_literal_method, [self_idx, name_idx, cc_idx], line, col)

      cc_idx
    end

    def on_string(node, current_cc)
      string = node.children[0]

      unless current_cc.strings.include?(string)
        current_cc.strings.add(string)
      end

      idx = current_cc.strings.get(string)
      target = current_cc.next_register

      current_cc
        .add_instruction(:set_string, [target, idx], node.line, node.column)

      target
    end

    def on_send(node, current_cc)
      rec, name, *args = *node

      if name.start_with?('__') and Instruction::NAME_MAPPING.key?(name[2..-1].to_sym)
        return on_send_instruction(node, current_cc)
      end

      unless current_cc.strings.include?(name)
        current_cc.strings.add(name)
      end

      name_idx = current_cc.strings.get(name)

      arg_indexes = args.map do |arg|
        process(arg, current_cc)
      end

      if rec
        rec_idx = process(rec, current_cc)
      else
        rec_idx = current_cc.next_register

        current_cc.add_instruction(:get_self, [rec_idx], node.line, node.column)
      end

      target = current_cc.next_register

      # TODO: properly determine visibility
      current_cc.add_instruction(
        :send,
        [target, rec_idx, name_idx, 1, arg_indexes.length, *arg_indexes],
        node.line,
        node.column
      )

      target
    end

    def on_send_instruction(node, current_cc)
      _, name, *args = *node

      ins_name = name[2..-1].to_sym

      ins_args = args.map { |node| node.children[0] }

      current_cc.add_instruction(ins_name, ins_args, node.line, node.column)
    end

    def on_integer(node, current_cc)
      int = node.children[0]

      unless current_cc.integers.include?(int)
        current_cc.integers.add(int)
      end

      target = current_cc.next_register
      idx = current_cc.integers.get(int)

      current_cc
        .add_instruction(:set_integer, [target, idx], node.line, node.column)

      target
    end
  end
end
