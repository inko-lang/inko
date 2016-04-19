module Aeon
  class Compiler
    CLASS_PROTOTYPE = '__prototype'

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

      last_ins.arguments[0]
    end

    def on_def(node, current_cc)
      rec, name, vis, _, args, _, body = *node

      unless current_cc.strings.include?(name)
        current_cc.strings.add(name)
      end

      req_args = args.children.count do |arg|
        arg.children[2].nil?
      end

      method = CompiledCode.new(name, @source_file, node.line, req_args, vis)

      args.children.each do |arg|
        method.locals.add(arg.children[0])
      end

      process(body, method)

      # Ensure a method always has a return instruction
      last_ins = method.instructions.last

      unless last_ins.name == :return
        method
          .ins_return([last_ins.arguments[0]], last_ins.line, last_ins.column)
      end

      line = node.line
      col  = node.column

      # TODO: support explicit receivers properly
      # TODO: support implicit receivers properly
      if rec
        rec_idx = process(rec, current_cc)
      else
        rec_idx = current_cc.next_register

        case current_cc.type
        when :class
          self_idx  = current_cc.next_register
          attr_name = current_cc.strings.add(CLASS_PROTOTYPE)

          current_cc
            .ins_get_self([self_idx], line, col)
            .ins_get_literal_attr([rec_idx, self_idx, attr_name], line, col)
        when :enum
        when :trait
        # Method defined at the top-level
        else
          current_cc.ins_get_self([rec_idx], line, col)
        end
      end

      cc_idx   = current_cc.code_objects.add(method)
      name_idx = current_cc.strings.get(name)

      current_cc.ins_def_literal_method([rec_idx, name_idx, cc_idx], line, col)

      cc_idx
    end

    def on_class(node, current_cc)
      name, parent, body = *node

      if name.type == :type
        name = name.children[0]
      end

      line = node.line
      col  = node.column

      if name.children[0]
        name_source = process(name.children[0], current_cc)
      else
        name_source = current_cc.next_register

        current_cc.ins_get_self([name_source], line, col)
      end

      parent_reg       = current_cc.next_register
      parent_class_reg = current_cc.next_register
      proto_name_idx   = current_cc.strings.add(CLASS_PROTOTYPE)

      if parent
        if parent.children[0]
          psource = process(parent.children[0], current_cc)
        else
          psource = current_cc.next_register

          current_cc.ins_get_self([psource], line, col)
        end

        parent_name = current_cc.strings.add(parent.children[1])

        current_cc
          .ins_get_literal_const([parent_class_reg, psource, parent_name], line, col)
          .ins_get_literal_attr([parent_reg, parent_class_reg, proto_name_idx], line, col)
      else
        idx = current_cc.next_register
        parent_name = current_cc.strings.add('Object')

        current_cc
          .ins_get_self([idx], line, col)
          .ins_get_literal_const([parent_class_reg, idx, parent_name], line, col)
          .ins_get_literal_attr([parent_reg, parent_class_reg, proto_name_idx], line, col)
      end

      name_str = name.children[1]
      name_idx = current_cc.strings.add(name_str)

      exists_reg = current_cc.next_register
      target_reg = current_cc.next_register
      proto_reg  = current_cc.next_register

      current_cc.ins_literal_const_exists([exists_reg, name_source, name_idx],
                                          line, col)

      jump_to = current_cc.label

      current_cc
        .ins_goto_if_true([jump_to, exists_reg], line, col)
        .ins_set_object([target_reg], line, col)
        .ins_set_object([proto_reg], line, col)
        .ins_set_prototype([proto_reg, parent_reg], line, col)
        .ins_set_literal_attr([target_reg, proto_reg, proto_name_idx], line, col)
        .ins_set_literal_const([name_source, target_reg, name_idx], line, col)
        .ins_get_literal_const([target_reg, name_source, name_idx], line, col)

      current_cc.mark_label(jump_to)

      body_code = CompiledCode
        .new(name_str, @source_file, line, 0, :public, :class)

      process(body, body_code)

      body_idx     = current_cc.code_objects.add(body_code)
      body_ret_idx = current_cc.next_register

      current_cc
        .ins_run_literal_code([body_ret_idx, body_idx, target_reg], line, col)

      target_reg
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

        current_cc.ins_get_self([rec_idx], node.line, node.column)
      end

      target = current_cc.next_register

      # TODO: properly determine visibility
      current_cc.ins_send_literal(
        [target, rec_idx, name_idx, 1, arg_indexes.length, *arg_indexes],
        node.line,
        node.column
      )

      target
    end

    def on_send_instruction(node, current_cc)
      _, name, *args = *node

      ins_name = name[2..-1].to_sym

      ins_args = []

      ins_args = args.map do |node|
        if node.type == :integer
          node.children[0]
        elsif node.type == :sstring or node.type == :dstring
          current_cc.strings.add(node.children[0])
        elsif node.type == :ident and node.children[1] == '_'
          current_cc.next_register
        else
          process(node, current_cc)
        end
      end

      current_cc.instruction(ins_name, ins_args, node.line, node.column)

      ins_args[0]
    end

    def on_return(node, current_cc)
      ret_idx = process(node.children[0], current_cc)

      current_cc.ins_return([ret_idx], node.line, node.column)

      ret_idx
    end

    def on_let(node, current_cc)
      var_node, _, val_node = *node

      val_idx = process(val_node, current_cc)

      line = node.line
      col  = node.column

      case var_node.type
      when :ident
        name     = var_node.children[1]
        name_idx = current_cc.locals.add(name)

        current_cc.ins_set_local([name_idx, val_idx], line, col)

        name_idx
      when :const
        name     = var_node.children[1]
        name_idx = current_cc.strings.add(name)
        self_idx = current_cc.next_register

        current_cc
          .ins_get_self([self_idx], line, col)
          .ins_set_literal_const([self_idx, val_idx, name_idx], line, col)

        name_idx
      when :ivar

      end
    end

    def on_self(node, current_cc)
      idx = current_cc.next_register

      current_cc.ins_get_self([idx], node.line, node.column)

      idx
    end

    def on_ident(node, current_cc)
      # TODO: determine when to use a lvar and when to use a send
      rec, name = *node

      local_idx = current_cc.locals.get_or_set(name)
      register  = current_cc.next_register

      line = node.line
      col  = node.column

      current_cc.ins_get_local([register, local_idx], line, col)

      register
    end

    def on_const(node, current_cc)
      rec, name = *node

      register = current_cc.next_register
      name_idx = current_cc.strings.get_or_set(name)

      line = node.line
      col  = node.column

      if rec
        rec_idx = process(rec, current_cc)
      else
        rec_idx = current_cc.next_register

        current_cc.ins_get_self([rec_idx], line, col)
      end

      current_cc.ins_get_literal_const([register, rec_idx, name_idx], line, col)

      register
    end

    def on_sstring(node, current_cc)
      string(node, current_cc)
    end

    def on_dstring(node, current_cc)
      string(node, current_cc, true)
    end

    def on_integer(node, current_cc)
      int = node.children[0]

      unless current_cc.integers.include?(int)
        current_cc.integers.add(int)
      end

      target = current_cc.next_register
      idx    = current_cc.integers.get(int)

      current_cc.ins_set_integer([target, idx], node.line, node.column)

      target
    end

    def on_float(node, current_cc)
      int = node.children[0]

      unless current_cc.floats.include?(int)
        current_cc.floats.add(int)
      end

      target = current_cc.next_register
      idx    = current_cc.floats.get(int)

      current_cc.ins_set_float([target, idx], node.line, node.column)

      target
    end

    private

    def string(node, current_cc, double_quote = false)
      # TODO: is this the best way of supporting escape sequences?
      string = node.children[0]

      if double_quote
        string.gsub!(/\\r|\\n|\\t/, '\n' => "\n", '\r' => "\r", '\t' => "\t")
      end

      unless current_cc.strings.include?(string)
        current_cc.strings.add(string)
      end

      idx = current_cc.strings.get(string)
      target = current_cc.next_register

      current_cc
        .instruction(:set_string, [target, idx], node.line, node.column)

      target
    end
  end
end
