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

        set_local(self_local, mod_reg, body, @module.location)
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
        true_reg = get_true(body, loc)
        mod = set_object(@module.type, true_reg, proto, body, loc)

        set_literal_attribute(modules, @module.name.to_s, mod, true, body, loc)
      end

      def on_import(node, body)
        qname = node.qualified_name
        location = node.location
        imported_mod = @state.module(qname)
        import_path = imported_mod.bytecode_import_path
        path_reg = set_string(import_path, body, location)

        body.instruct(:LoadModule, body.register_dynamic, path_reg, location)

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
          body.instruct(:Return, ins.register, loc)
        elsif !ins
          body.instruct(:Return, get_nil(body, loc), loc)
        end
      end

      def check_for_unreachable_blocks(body)
        body.blocks.each do |block|
          next if body.reachable_basic_block?(block)

          diagnostics.unreachable_code_warning(block.location)
        end
      end

      def on_integer(node, body)
        register = body.register(typedb.integer_type)

        body.instruct(:SetInteger, register, node.value, node.location)
      end

      def on_float(node, body)
        register = body.register(typedb.float_type)

        body.instruct(:SetFloat, register, node.value, node.location)
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
          get_local(body.locals[name], body, loc)
        elsif body.self_type.responds_to_message?(name)
          send_to_self(name, body, loc)
        elsif @module.globals.defined?(name)
          get_global(@module.globals[name], body, loc)
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

        define_block(
          name,
          type,
          body.self_type,
          node.arguments,
          node.body,
          body,
          location
        )
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
        rec_type = receiver.type
        block_reg = define_block(
          name,
          rec_type.lookup_method(name).type,
          rec_type,
          node.arguments,
          node.body,
          body,
          location
        )

        set_literal_attribute(receiver, name, block_reg, false, body, location)
      end

      def define_block(
        name,
        type,
        self_type,
        arguments,
        block_body,
        body,
        location
      )
        code_object = body.add_code_object(name, type, location)

        code_object.define_self_local(self_type)

        define_block_arguments(code_object, arguments)

        on_body(block_body, code_object)

        body.instruct(:SetBlock, body.register(type), code_object, location)
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
        exists_reg = local_exists(local, location)

        body.instruct(:GotoNextBlockIfTrue, exists_reg, location)

        set_local(local, process_node(vnode, body), body, location)
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

        set_local(symbol, value, body, variable.location)
      end

      def set_local(symbol, value, body, location)
        body.instruct(:SetLocal, symbol, value, location)
      end

      def get_local(symbol, body, location)
        register = body.register(symbol.type)

        body.instruct(:GetLocal, register, symbol, location)
      end

      def get_global(symbol, body, location)
        register = body.register(symbol.type)

        body.instruct(:GetGlobal, register, symbol, location)
      end

      def local_exists(symbol, location)
        register = body.register(typedb.boolean_type)

        body.instruct(:LocalExists, register, symbol, location)
      end

      def on_define_attribute(variable, value, mutable, body)
        loc = variable.location
        name = variable.name
        receiver = get_self(body, loc)

        set_literal_attribute(receiver, name, value, mutable, body, loc)
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

        set_attribute(receiver, name, value, body, node.location)
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

        set_object(type, permanent, prototype, body, loc)
      end

      def on_raw_get_true(node, body)
        get_true(body, node.location)
      end

      def send_to_self(name, body, location)
        receiver = get_self(body, location)

        send_object_message(receiver, name, [], body, location)
      end

      def get_toplevel(body, location)
        register = body.register(typedb.top_level)

        body.instruct(:GetToplevel, register, location)
      end

      def get_self(body, location)
        get_local(body.self_local, body, location)
      end

      def get_nil(body, location)
        register = body.register(typedb.nil_type)

        body.instruct(:GetNil, register, location)
      end

      def get_true(body, location)
        register = body.register(typedb.boolean_type)

        body.instruct(:GetTrue, register, location)
      end

      def set_string(value, body, location)
        register = body.register(typedb.string_type)

        body.instruct(:SetString, register, value, location)
      end

      def send_object_message(receiver, name, arguments, body, location)
        rec_type = receiver.type
        reg_type = rec_type.message_return_type(name)
        reg = body.register(reg_type)
        name_reg = set_string(name, body, location)

        unless rec_type.responds_to_message?(name)
          diagnostics.undefined_method_error(rec_type, name, location)
        end

        args = [receiver] + arguments

        body
          .instruct(:SendObjectMessage, reg, receiver, name_reg, args, location)
      end

      def get_attribute(receiver, name, body, location)
        rec_type = receiver.type
        symbol = rec_type.lookup_attribute(name)
        name_reg = set_string(name, body, location)

        unless symbol.any?
          diagnostics.undefined_attribute_error(rec_type, name, location)
        end

        register = body.register(symbol.type)

        body.instruct(:GetAttribute, register, receiver, name_reg, location)
      end

      def set_attribute(receiver, name, value, body, location)
        register = body.register(value.type)

        body.instruct(:SetAttribute, register, receiver, name, value, location)
      end

      def set_literal_attribute(receiver, name, value, mutable, body, location)
        name_reg = set_string(name, body, location)
        rec_type = receiver.type

        # TODO: type inference should handle this.
        if rec_type.lookup_attribute(name).nil?
          rec_type.define_attribute(name, value.type, mutable)
        end

        set_attribute(receiver, name_reg, value, body, location)
      end

      def set_object(type, permanent, prototype, body, location)
        register = body.register(type)

        body.instruct(:SetObject, register, permanent, prototype, location)
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
