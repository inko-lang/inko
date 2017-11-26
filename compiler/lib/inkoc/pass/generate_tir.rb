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
        return if diagnostics.errors?

        on_module_body(ast, @module.body)

        []
      end

      def process_imports(body)
        body.add_connected_basic_block('imports')

        mod_regs = load_modules(body)

        @module.imports.each do |import|
          register = mod_regs[import.qualified_name.to_s]

          process_node(import, register, body)
        end
      end

      def load_modules(body)
        imported = Set.new
        registers = {}

        @module.imports.each do |import|
          qname = import.qualified_name

          next if imported.include?(qname.to_s)

          load_module(qname, body, import.location)

          registers[qname.to_s] =
            set_module_register(qname, body, import.location)

          imported << qname.to_s
        end

        registers
      end

      def load_module(qname, body, location)
        imported_mod = @state.module(qname)
        import_path = imported_mod.bytecode_import_path
        path_reg = set_string(import_path, body, location)

        body.instruct(:LoadModule, body.register_dynamic, path_reg, location)
      end

      def set_module_register(qname, body, location)
        top_reg = get_toplevel(body, location)
        mods_reg =
          get_attribute(top_reg, Config::MODULES_ATTRIBUTE, body, location)

        get_attribute(mods_reg, qname.to_s, body, location)
      end

      def on_module_body(node, body)
        define_module(body)
        process_imports(@module.body)
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

      def on_import(import, mod_reg, body)
        source_mod = @state.module(import.qualified_name)

        import.symbols.each do |symbol|
          process_node(symbol, source_mod, mod_reg, body)
        end
      end

      def on_import_symbol(symbol, source_mod, mod_reg, body)
        return unless symbol.expose?

        import_as = symbol.import_as(source_mod)
        loc = symbol.location
        symbol_reg = get_attribute(mod_reg, symbol.symbol_name.name, body, loc)

        set_global(import_as, symbol_reg, body, loc)
      end

      def on_import_self(symbol, source_mod, mod_reg, body)
        return unless symbol.expose?

        import_as = symbol.import_as(source_mod)

        set_global(import_as, mod_reg, body, symbol.location)
      end

      def on_import_glob(symbol, source_mod, mod_reg, body)
        loc = symbol.location

        source_mod.attributes.each do |attribute|
          sym_name = attribute.name
          symbol_reg = get_attribute(mod_reg, sym_name, body, loc)

          set_global(sym_name, symbol_reg, body, loc)
        end
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
        register = body.register(typedb.integer_instance)

        body.instruct(:SetInteger, register, node.value, node.location)
      end

      def on_float(node, body)
        register = body.register(typedb.float_instance)

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

        source =
          if node.receiver
            process_node(node.receiver, body)
          else
            get_self(body, loc)
          end

        if source.type.lookup_attribute(name).any?
          get_attribute(source, name, body, loc)
        elsif !node.receiver && @module.globals.defined?(name)
          get_global(name, body, loc)
        else
          get_nil(body, loc)
        end
      end

      def on_global(node, body)
        get_global(node.name, body, node.location)
      end

      def on_method(node, body)
        return if node.required?

        receiver = get_self(body, node.location)

        define_method(node, receiver, body)
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

          # TODO: support rest argument defaults
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

      def on_reopen_object(node, body)
        loc = node.location
        object = get_global(node.name.name, body, loc)

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
        depth, symbol = body.locals.lookup_with_parent(name)
        register = body.register(symbol.type)

        if depth.positive?
          body.instruct(:GetParentLocal, register, depth, symbol, location)
        else
          body.instruct(:GetLocal, register, symbol, location)
        end
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
        register = body.register(Type::Dynamic.new)

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

      def on_reassign_local(variable, value, body)
        name = variable.name
        loc = variable.location
        depth, symbol = body.locals.lookup_with_parent(name)

        if depth.positive?
          body.instruct(:SetParentLocal, symbol, depth, value, loc)
        else
          set_local(symbol, value, body, loc)
        end
      end

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

      def on_raw_set_prototype(node, body)
        object = process_node(node.arguments.fetch(0), body)
        proto = process_node(node.arguments.fetch(1), body)

        body.instruct(:SetPrototype, object, proto, node.location)
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
        register = body.register(typedb.string_instance)
        value = process_node(node.arguments.fetch(0), body)

        body.instruct(:IntegerToString, register, value, node.location)
      end

      def on_raw_stdout_write(node, body)
        register = body.register(typedb.integer_instance)
        value = process_node(node.arguments.fetch(0), body)

        body.instruct(:StdoutWrite, register, value, node.location)
      end

      def on_raw_get_true(node, body)
        get_true(body, node.location)
      end

      def on_raw_get_false(node, body)
        get_false(body, node.location)
      end

      def on_raw_get_nil(node, body)
        get_nil(body, node.location)
      end

      def on_raw_run_block(node, body)
        block = process_node(node.arguments.fetch(0), body)
        self_reg = get_self(body, node.location)
        arguments = process_nodes(node.arguments[1..-1], body)

        run_block(block, [self_reg, *arguments], body, node.location)
      end

      def on_raw_get_string_prototype(node, body)
        register = body.register(typedb.string_type)

        body.instruct(:GetStringPrototype, register, node.location)
      end

      def on_raw_get_integer_prototype(node, body)
        register = body.register(typedb.integer_type)

        body.instruct(:GetIntegerPrototype, register, node.location)
      end

      def on_raw_get_float_prototype(node, body)
        register = body.register(typedb.float_type)

        body.instruct(:GetFloatPrototype, register, node.location)
      end

      def on_raw_get_array_prototype(node, body)
        register = body.register(typedb.array_type)

        body.instruct(:GetArrayPrototype, register, node.location)
      end

      def on_raw_get_block_prototype(node, body)
        register = body.register(typedb.block_type)

        body.instruct(:GetBlockPrototype, register, node.location)
      end

      def on_raw_array_length(node, body)
        register = body.register(typedb.integer_instance)
        array_register = process_node(node.arguments.fetch(0), body)

        body.instruct(:ArrayLength, register, array_register, node.location)
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

        body.instruct(:Throw, register, node.location)

        get_nil(body, node.location)
      end

      def on_try(node, body)
        catch_reg = body.register(body.type.throws)
        ret_reg = body.register(node.expression.type)

        # Block for running the to-try expression
        try_block = body.add_connected_basic_block
        try_reg = process_node(node.expression, body)

        body.instruct(:SetRegister, ret_reg, try_reg, node.location)
        body.instruct(:SkipNextBlock, node.location)

        # Block for error handling
        else_block = body.add_connected_basic_block
        else_reg = register_for_else_block(node, body, catch_reg)

        body.instruct(:SetRegister, ret_reg, else_reg, node.location)

        # Block for everything that comes after our "try" expression.
        body.add_connected_basic_block

        body.catch_table << TIR::CatchEntry
          .new(try_block, else_block, catch_reg)

        ret_reg
      end

      def register_for_else_block(node, body, catch_reg)
        if node.explicit_block_for_else_body?
          block_reg = define_block_for_else(node, body)
          self_reg = get_self(body, node.else_body.location)
          else_loc = node.else_body.location

          run_block(block_reg, [self_reg, catch_reg], body, else_loc)
        else
          process_nodes(node.else_body.expressions, body).last ||
            get_nil(body, node.else_body.location)
        end
      end

      def define_block_for_else(node, body)
        location = node.else_body.location
        block_type = node.else_block_type

        else_code = body.add_code_object(
          block_type.name,
          block_type,
          location,
          locals: node.else_body.locals
        )

        on_body(node.else_body, else_code)

        body.instruct(:SetBlock, body.register(block_type), else_code, location)
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
        register = body.register(typedb.true_type)

        body.instruct(:GetTrue, register, location)
      end

      def get_false(body, location)
        register = body.register(typedb.false_type)

        body.instruct(:GetFalse, register, location)
      end

      def set_string(value, body, location)
        register = body.register(typedb.string_instance)

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
