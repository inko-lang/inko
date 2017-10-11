# frozen_string_literal: true

module Inkoc
  module Pass
    class GenerateTir
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

        loc = @module.location

        mod_reg = value_for_module_self(body)
        mod_reg = set_global(Config::MODULE_GLOBAL, mod_reg, body, loc)

        set_local(body.self_local, mod_reg, body, loc)
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
        mod = set_object_with_prototype(body.type, true_reg, proto, body, loc)

        set_literal_attribute(modules, @module.name.to_s, mod, body, loc)
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

        registers
      end

      def add_explicit_return(body)
        return unless body.reachable_basic_block?(body.current_block)

        ins = body.current_block.instructions.last
        loc = ins ? ins.location : body.location

        if ins && !ins.return?
          body.instruct(:Return, ins.register, loc)
        elsif !ins
          body.instruct(:Return, get_nil(body, loc), loc)
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
          get_local(name, body, loc)
        elsif body.self_type.responds_to_message?(name)
          send_to_self(name, body, loc)
        elsif @module.responds_to_message?(name)
          send_object_message(
            get_global(Config::MODULE_GLOBAL, body, loc),
            name,
            [],
            body,
            loc
          )
        elsif @module.global_defined?(name)
          get_global(name, body, loc)
        else
          get_nil(body, loc)
        end
      end

      def on_attribute(node, body)
        loc = node.location

        get_attribute(get_self(body, loc), node.name, body, loc)
      end

      def on_constant(node, body)
        name = node.name
        loc = node.location
        self_type = body.self_type

        if self_type.lookup_attribute(name).any?
          get_attribute(get_self(body, loc), name, body, loc)
        elsif @module.globals.defined?(name)
          get_global(name, body, loc)
        else
          get_nil(body, loc)
        end
      end

      def on_global(node, body)
        get_global(node.name, body, node.location)
      end

      def on_method(node, body)
        receiver = get_self(body, node.location)

        if node.required?
          # TODO: figure out how to expose required methods to the runtime
          # define_required_method(node, receiver, body)
        else
          define_method(node, receiver, body)
        end
      end

      def on_block(node, body)
        define_block(
          Config::BLOCK_NAME,
          node.type,
          node.arguments,
          node.body,
          node.body.locals,
          body,
          node.location
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
        block_reg = define_block(
          name,
          node.type,
          node.arguments,
          node.body,
          node.body.locals,
          body,
          location
        )

        block_reg =
          set_global_if_module_scope(receiver, name, block_reg, body, location)

        set_literal_attribute(receiver, name, block_reg, body, location)
      end

      def define_block(
        name,
        type,
        arguments,
        block_body,
        locals,
        body,
        location
      )
        code_object = body.add_code_object(name, type, location, locals: locals)

        define_block_arguments(code_object, arguments)

        on_body(block_body, code_object)

        body.instruct(:SetBlock, body.register(type), code_object, location)
      end

      def define_block_arguments(code_object, arguments)
        arguments.each do |arg|
          symbol = code_object.type.lookup_argument(arg.name)
          local = code_object.define_immutable_local(arg.name, symbol.type)

          next unless arg.default

          define_argument_default(code_object, local, arg.default)
        end
      end

      def define_argument_default(body, local, vnode)
        body.add_connected_basic_block("#{local.name}_default")

        location = vnode.location
        exists_reg = local_exists(local, body, location)

        body.instruct(:GotoNextBlockIfTrue, exists_reg, location)

        set_local(local, process_node(vnode, body), body, location)
      end

      def on_object(node, body)
        define_object(node, body, Config::OBJECT_CONST)
      end

      def on_trait(node, body)
        define_object(node, body, Config::TRAIT_CONST)
      end

      def define_object(node, body, proto_name)
        name = node.name
        loc = node.location
        type = body.self_type.lookup_attribute(name).type
        true_reg = get_true(body, loc)

        object =
          if type.prototype
            top = get_toplevel(body, loc)
            proto = get_attribute(top, proto_name, body, loc)

            set_object_with_prototype(type, true_reg, proto, body, loc)
          else
            set_object(type, true_reg, body, loc)
          end

        object = store_object_literal(object, name, body, loc)

        set_object_literal_name(object, name, body, loc)

        block = define_block(
          name,
          node.block_type,
          [],
          node.body,
          node.body.locals,
          body,
          loc
        )

        run_block(block, [object], body, loc)

        object
      end

      def on_trait_implementation(node, body)
        loc = node.location
        object = get_global(node.object_name.name, body, loc)
        trait = get_global(node.trait_name.name, body, loc)

        send_object_message(
          object,
          Config::IMPLEMENT_TRAIT_MESSAGE,
          [trait],
          body,
          loc
        )

        block = define_block(
          Config::IMPL_NAME,
          node.block_type,
          [],
          node.body,
          node.body.locals,
          body,
          loc
        )

        run_block(block, [object], body, loc)

        object
      end

      def trait_builtin(body, location)
        top = get_toplevel(body, location)

        get_attribute(top, Config::TRAIT_CONST, body, location)
      end

      def set_object_literal_name(object, name, body, location)
        attr = Config::OBJECT_NAME_INSTANCE_ATTRIBUTE
        name_reg = set_string(name, body, location)

        set_literal_attribute(object, attr, name_reg, body, location)
      end

      def store_object_literal(object, name, body, location)
        receiver = get_self(body, location)

        object =
          set_global_if_module_scope(receiver, name, object, body, location)

        set_literal_attribute(receiver, name, object, body, location)
      end

      def on_send(node, body)
        location = node.location
        receiver = receiver_for_send(node, body)
        arg_regs = process_nodes(node.arguments, body)

        send_object_message(receiver, node.name, arg_regs, body, location)
      end

      def receiver_for_send(node, body)
        if node.receiver
          process_node(node.receiver, body)
        elsif node.receiver_type == @module.type
          get_global(Config::MODULE_GLOBAL, body, node.location)
        else
          get_self(body, location)
        end
      end

      def on_define_variable(node, body)
        callback = node.variable.define_variable_visitor_method
        value = process_node(node.value, body)

        public_send(callback, node.variable, value, body)
      end

      def on_define_local(variable, value, body)
        name = variable.name
        symbol = body.locals[name]

        set_local(symbol, value, body, variable.location)
      end

      def set_local(symbol, value, body, location)
        body.instruct(:SetLocal, symbol, value, location)
      end

      def get_local(name, body, location)
        symbol = body.locals[name]
        register = body.register(symbol.type)

        body.instruct(:GetLocal, register, symbol, location)
      end

      def set_global_if_module_scope(receiver, name, value, body, location)
        if module_scope?(receiver.type)
          set_global(name, value, body, location)
        else
          value
        end
      end

      def set_global(name, value, body, location)
        symbol = @module.globals[name]
        register = body.register(symbol.type)

        body.instruct(:SetGlobal, register, symbol, value, location)
      end

      def get_global(name, body, location)
        symbol = @module.globals[name]
        register = body.register(symbol.type)

        body.instruct(:GetGlobal, register, symbol, location)
      end

      def local_exists(symbol, body, location)
        register = body.register(typedb.boolean_type)

        body.instruct(:LocalExists, register, symbol, location)
      end

      def on_define_attribute(variable, value, body)
        loc = variable.location
        name = variable.name
        receiver = get_self(body, loc)

        set_literal_attribute(receiver, name, value, body, loc)
      end

      def on_define_constant(variable, value, body)
        loc = variable.location
        name = variable.name
        receiver = get_self(body, loc)
        value = set_global_if_module_scope(receiver, name, value, body, loc)

        set_literal_attribute(receiver, name, value, body, loc)
      end

      def on_reassign_variable(node, body)
        callback = node.variable.reassign_variable_visitor_method
        value = process_node(node.value, body)

        public_send(callback, node.variable, value, body)
      end

      alias on_reassign_local on_define_local
      alias on_reassign_attribute on_define_attribute

      def on_raw_instruction(node, body)
        callback = node.raw_instruction_visitor_method

        if respond_to?(callback)
          public_send(callback, node, body)
        else
          get_nil(body, node.location)
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
          proto = process_node(args[1], body)

          set_object_with_prototype(node.type, permanent, proto, body, loc)
        else
          set_object(node.type, permanent, body, loc)
        end
      end

      def on_raw_integer_to_string(node, body)
        register = body.register(typedb.string_type)
        value = process_node(node.arguments.fetch(0), body)

        body.instruct(:IntegerToString, register, value, node.location)
      end

      def on_raw_stdout_write(node, body)
        register = body.register(typedb.integer_type)
        value = process_node(node.arguments.fetch(0), body)

        body.instruct(:StdoutWrite, register, value, node.location)
      end

      def on_raw_get_true(node, body)
        get_true(body, node.location)
      end

      def on_raw_get_false(node, body)
        get_false(body, node.location)
      end

      def on_return(node, body)
        location = node.location
        value =
          if node.value
            process_node(node.value, body)
          else
            get_nil(body, location)
          end

        register = body.register(value.type)

        body.instruct(:Return, register, location)
        body.add_basic_block
      end

      def on_throw(node, body)
        register = process_node(node.value, body)

        body.instruct(:Throw, register, :throw, node.location)
        body.add_basic_block
      end

      def run_block(block, arguments, body, location)
        register = body.register(block.type.return_type)

        body.instruct(:RunBlock, register, block, arguments, location)
      end

      def send_to_self(name, body, location)
        send_object_message(get_self(body, location), name, [], body, location)
      end

      def get_toplevel(body, location)
        register = body.register(typedb.top_level)

        body.instruct(:GetToplevel, register, location)
      end

      def get_self(body, location)
        get_local(Config::SELF_LOCAL, body, location)
      end

      def get_nil(body, location)
        register = body.register(typedb.nil_type)

        body.instruct(:GetNil, register, location)
      end

      def get_true(body, location)
        register = body.register(typedb.boolean_type)

        body.instruct(:GetTrue, register, location)
      end

      def get_false(body, location)
        register = body.register(typedb.boolean_type)

        body.instruct(:GetFalse, register, location)
      end

      def set_string(value, body, location)
        register = body.register(typedb.string_type)

        body.instruct(:SetString, register, value, location)
      end

      def send_object_message(receiver, name, arguments, body, location)
        rec_type = receiver.type
        reg = body.register(rec_type.message_return_type(name))
        name_reg = set_string(name, body, location)
        args = [receiver] + arguments

        body
          .instruct(:SendObjectMessage, reg, receiver, name_reg, args, location)
      end

      def get_attribute(receiver, name, body, location)
        rec_type = receiver.type
        symbol = rec_type.lookup_attribute(name)
        name_reg = set_string(name, body, location)
        register = body.register(symbol.type)

        body.instruct(:GetAttribute, register, receiver, name_reg, location)
      end

      def set_attribute(receiver, name, value, body, location)
        register = body.register(value.type)

        body.instruct(:SetAttribute, register, receiver, name, value, location)
      end

      def set_literal_attribute(receiver, name, value, body, location)
        name_reg = set_string(name, body, location)

        set_attribute(receiver, name_reg, value, body, location)
      end

      def set_object(type, permanent, body, location)
        register = body.register(type)

        body.instruct(:SetObject, register, permanent, nil, location)
      end

      def set_object_with_prototype(type, permanent, prototype, body, location)
        register = body.register(type)

        body.instruct(:SetObject, register, permanent, prototype, location)
      end

      def diagnostics
        @state.diagnostics
      end

      def typedb
        @state.typedb
      end

      def module_scope?(self_type)
        self_type == @module.type
      end

      def inspect
        # The default inspect is very slow, slowing down the rendering of any
        # runtime errors.
        '#<Pass::GenerateTir>'
      end
    end
  end
end
