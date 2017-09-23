# frozen_string_literal: true

module Inkoc
  module Pass
    class ModuleBody
      include TypeVerification
      include VisitorMethods

      def initialize(mod, state)
        @module = mod
        @state = state
      end

      def run(ast)
        process_imports(@module.body)
        on_module_body(ast, @module.body)

        []
      end

      def process_imports(body)
        body.add_connected_basic_block('imports')

        @module.imports.each do |import|
          on_import(import, body)
        end
      end

      def on_module_body(node, body)
        define_module(body)
        process_node(node, body)
      end

      def define_module(body)
        body.add_connected_basic_block('define_module')

        mod_reg = value_for_module_self(body)
        self_local = body.define_self_local(mod_reg.type)

        body.set_local(self_local, mod_reg, @module.location)
      end

      def value_for_module_self(body)
        if @module.define_module?
          define_module_object(body)
        else
          get_toplevel(body, @module.location)
        end
      end

      def define_module_object(body)
        loc = @module.location
        top = get_toplevel(body, loc)

        # Get the object containing all modules (Inko::modules).
        modules = get_attribute(top, Config::MODULES_ATTRIBUTE, body, loc)

        # Get the prototype for the new module (Inko::Module)
        proto = get_attribute(top, Config::MODULE_TYPE, body, loc)

        # Create the new module and store it in the modules list.
        true_reg = body.get_true(typedb.boolean_type, loc)
        mod = body.set_object(@module.type, true_reg, proto, loc)

        set_attribute(modules, @module.name.to_s, mod, true, body, loc)
      end

      def on_import(node, body)
        qname = node.qualified_name
        location = node.location
        imported_mod = @state.module(qname)
        import_path = imported_mod.bytecode_import_path
        path_reg = set_string(import_path, body, location)

        body.load_module(path_reg, location)

        # TODO: import symbols
      end

      def on_body(node, body)
        body.add_connected_basic_block

        registers = process_nodes(node.expressions, body)

        add_explicit_return(body)
        check_for_unreachable_blocks(body)

        registers
      end

      def add_explicit_return(body)
        ins = body.current_block.instructions.last
        loc = ins ? ins.location : body.location

        if ins && !ins.return?
          body.return_value(ins.register, loc)
        elsif !ins
          body.return_value(get_nil(body, loc), loc)
        end
      end

      def check_for_unreachable_blocks(body)
        body.blocks.each do |block|
          next if body.reachable_basic_block?(block)

          diagnostics.unreachable_code_warning(block.location)
        end
      end

      def on_integer(node, body)
        body.set_integer(node.value, typedb.integer_type, node.location)
      end

      def on_float(node, body)
        body.set_float(node.value, typedb.float_type, node.location)
      end

      def on_string(node, body)
        set_string(node.value, body, node.location)
      end

      def on_self(node, body)
        get_self(body, node.location)
      end

      def on_identifier(node, body)
        name = node.name
        loc = node.location

        if body.locals.defined?(name)
          body.get_local(body.locals[name], loc)
        elsif body.self_type.responds_to_message?(name)
          send_to_self(name, body, loc)
        elsif @module.globals.defined?(name)
          body.get_global(@module.globals[name], loc)
        else
          diagnostics.undefined_method_error(body.self_type, name, loc)
          get_nil(body, loc)
        end
      end

      def on_attribute(node, body)
        name = node.name
        loc = node.location
        receiver = get_self(body, loc)

        get_attribute(receiver, name, body, loc)
      end

      alias on_constant on_attribute

      def on_method(node, body)
        receiver = get_self(body, node.location)

        if node.required?
          define_required_method(node, receiver, body) if receiver.type.trait?
        else
          define_method(node, receiver, body)
        end
      end

      def on_block(node, body)
        name = '<block>'
        location = node.location
        type = Type::Block.new(name, typedb.block_prototype)
        block = body.add_code_object(name, type, location)

        define_block_arguments(block, node.arguments)

        on_body(node.body, block)

        body.set_block(block, type, location)
      end

      def define_required_method(node, receiver, body)
        location = node.location
        msg_name =
          set_string(Config::DEFINE_REQUIRED_METHOD_MESSAGE, body, location)

        method_name = set_string(node.name, body, location)

        send_object_message(receiver, msg_name, [method_name], body, location)
      end

      def define_method(node, receiver, body)
        location = node.location
        name = node.name
        name_reg = set_string(name, body, location)
        type = receiver.type.lookup_method(name).type
        block = body.add_code_object(name, type, location)

        define_block_arguments(block, node.arguments)
        on_body(node.body, block)

        block_reg = body.set_block(block, type, location)

        body.set_attribute(receiver, name_reg, block_reg, location)
      end

      def define_block_arguments(block, arguments)
        arguments.each do |arg|
          symbol = block.type.lookup_argument(arg.name)
          local = block.define_immutable_local(arg.name, symbol.type)

          next unless arg.default

          define_argument_default(block, local, arg.default)
        end
      end

      def define_argument_default(body, local, vnode)
        body.add_connected_basic_block("#{local.name}_default")

        location = vnode.location
        exists_reg = body.local_exists(typedb.boolean_type, local, location)

        body.goto_next_block_if_true(exists_reg, location)
        body.set_local(local, process_node(vnode, body), location)
      end

      def on_send(node, body)
        return on_raw_instruction(node, body) if node.raw_instruction?

        location = node.location
        receiver =
          if node.receiver
            process_node(node.receiver, body)
          else
            get_self(body, location)
          end

        arg_regs = process_nodes(node.arguments, body)

        send_object_message(receiver, node.name, arg_regs, body, location)
      end

      def on_define_variable(node, body)
        callback = node.variable.define_variable_visitor_method
        value = process_node(node.value, body)

        public_send(callback, node.variable, value, node.mutable?, body)
      end

      def on_define_local(variable, value, mutable, body)
        name = variable.name
        symbol = body.define_local(name, value.type, mutable)

        body.set_local(symbol, value, variable.location)
      end

      def on_define_attribute(variable, value, mutable, body)
        loc = variable.location
        name = variable.name
        receiver = get_self(body, loc)

        set_attribute(receiver, name, value, mutable, body, loc)
      end

      alias on_define_constant on_define_attribute

      def on_raw_instruction(node, body)
        name = node.name
        callback = :"on_raw_#{name}"

        if respond_to?(callback)
          public_send(callback, node, body)
        else
          location = node.location

          diagnostics.unknown_raw_instruction_error(name, location)
          get_nil(body, location)
        end
      end

      def on_raw_get_toplevel(node, body)
        get_toplevel(body, node.location)
      end

      def on_raw_set_attribute(node, body)
        args = node.arguments
        receiver = process_node(args.fetch(0), body)
        name = process_node(args.fetch(1), body)
        value = process_node(args.fetch(2), body)

        body.set_attribute(receiver, name, value, node.location)
      end

      def on_raw_set_object(node, body)
        args = node.arguments
        loc = node.location
        permanent = process_node(args.fetch(0), body)

        if args[1]
          prototype = process_node(args[1], body)
          type = Type::Object.new(nil, prototype.type)
        else
          prototype = get_nil(body, loc)
          type = Type::Object.new
        end

        body.set_object(type, permanent, prototype, loc)
      end

      def on_raw_get_true(node, body)
        body.get_true(typedb.boolean_type, node.location)
      end

      def send_to_self(name, body, location)
        receiver = get_self(body, location)
        reg_type = receiver.type.message_return_type(name)
        reg = body.register(reg_type)
        name_reg = set_string(name, body, location)

        body.send_object_message(reg, receiver, name_reg, [], location)
      end

      def get_toplevel(body, location)
        body.get_toplevel(typedb.top_level, location)
      end

      def get_self(body, location)
        body.get_local(body.self_local, location)
      end

      def get_nil(body, location)
        body.get_nil(@state.typedb.nil_type, location)
      end

      def set_string(value, body, location)
        body.set_string(value, typedb.string_type, location)
      end

      def set_array_of_strings(values, body, location)
        value_regs = values.map { |v| set_string(v, body, location) }
        type = array_type

        body.set_array(value_regs, type, location)
      end

      def send_object_message(receiver, name, arguments, body, location)
        rec_type = receiver.type
        reg_type = rec_type.message_return_type(name)
        reg = body.register(reg_type)
        name_reg = set_string(name, body, location)

        unless rec_type.responds_to_message?(name)
          diagnostics.undefined_method_error(rec_type, name, location)
        end

        arguments = [receiver] + arguments

        body.send_object_message(reg, receiver, name_reg, arguments, location)
      end

      def get_attribute(receiver, name, body, location)
        rec_type = receiver.type
        symbol = rec_type.lookup_attribute(name)
        name_reg = set_string(name, body, location)

        unless symbol.any?
          diagnostics.undefined_attribute_error(rec_type, name, location)
        end

        body.get_attribute(receiver, name_reg, symbol.type, location)
      end

      def set_attribute(receiver, name, value, mutable, body, location)
        name_reg = set_string(name, body, location)
        rec_type = receiver.type

        # TODO: type inference should handle this.
        if rec_type.lookup_attribute(name).nil?
          rec_type.define_attribute(name, value.type, mutable)
        end

        body.set_attribute(receiver, name_reg, value, location)
      end

      def array_type
        typedb.array_prototype.new_instance
      end

      def diagnostics
        @state.diagnostics
      end

      def typedb
        @state.typedb
      end
    end
  end
end
