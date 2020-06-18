# frozen_string_literal: true

module Inkoc
  module Pass
    # rubocop: disable Metrics/ClassLength
    class GenerateTir
      include VisitorMethods

      def initialize(mod, state)
        @module = mod
        @state = state
      end

      def run(ast)
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

          registers[qname.to_s] = load_module(qname, body, import.location)

          imported << qname.to_s
        end

        registers
      end

      def load_module(qname, body, location)
        imported_mod = @state.module(qname)
        import_path = imported_mod.bytecode_import_path

        name_reg = set_string(qname.to_s, body, location)
        path_reg = set_string(import_path, body, location)
        reg = body.register(@module.type)

        body.instruct(:Binary, :ModuleLoad, reg, name_reg, path_reg, location)
      end

      def on_module_body(node, body)
        process_imports(@module.body)

        define_module(body)

        process_node(node, body)
      end

      def define_module(body)
        body.add_connected_basic_block('define_module')

        loc = @module.location

        mod_reg = body.register(@module.type)
        mod_name_reg = set_string(@module.name.to_s, body, loc)

        body.instruct(:Unary, :ModuleGet, mod_reg, mod_name_reg, loc)

        set_global(Config::MODULE_GLOBAL, mod_reg, body, loc)
      end

      def set_current_file_path(body, location)
        set_string(@module.location.file.path.to_s, body, location)
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

        ins = body.last_instruction
        loc = ins ? ins.location : body.location

        if ins
          body.instruct(:Return, false, ins.register, loc) unless ins.return?
        else
          body.instruct(:Return, false, get_nil(body, loc), loc)
        end
      end

      def on_integer(node, body)
        set_integer(node.value, body, node.location)
      end

      def on_float(node, body)
        register = body.register(typedb.float_type)

        body.instruct(:SetLiteral, register, node.value, node.location)
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

        if node.symbol && node.depth
          get_local_symbol(node.depth, node.symbol, body, loc)
        elsif body.self_type.responds_to_message?(name)
          send_to_self(name, node.block_type, node.type, body, loc)
        elsif @module.responds_to_message?(name)
          send_object_message(
            get_global(Config::MODULE_GLOBAL, body, loc),
            name,
            [],
            [],
            node.block_type,
            node.type,
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
        source = get_self(body, loc)

        if source.type.lookup_attribute(name).any?
          get_attribute(source, name, body, loc)
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

        define_method(node, receiver, body)
      end

      def on_block(node, body)
        define_block(
          node.block_name,
          node.type,
          node.arguments,
          node.body,
          node.body.locals,
          body,
          node.location
        )
      end

      def on_lambda(node, body)
        this_module = get_global(Config::MODULE_GLOBAL, body, node.location)

        define_block(
          node.block_name,
          node.type,
          node.arguments,
          node.body,
          node.body.locals,
          body,
          node.location,
          this_module
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
          location,
          receiver
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
        location,
        receiver = nil
      )
        code_object = body.add_code_object(name, type, location, locals: locals)

        define_block_arguments(code_object, arguments)

        on_body(block_body, code_object)

        body.instruct(
          :SetBlock,
          body.register(type),
          code_object,
          receiver,
          location
        )
      end

      def define_block_arguments(code_object, arguments)
        arguments.each do |arg|
          symbol = code_object.type.arguments[arg.name]

          if arg.default
            define_argument_default(code_object, symbol, arg)
          elsif arg.rest?
            define_rest_default(code_object, symbol, arg)
          end
        end
      end

      def define_argument_default(body, local, arg)
        generate_argument_default(body, local, arg.default.location) do
          process_node(arg.default, body)
        end
      end

      def define_rest_default(body, local, arg)
        generate_argument_default(body, local, arg.location) do
          array_reg = body.register(typedb.new_array_of_type(arg.type))

          body.instruct(:SetArray, array_reg, [], arg.location)
        end
      end

      def generate_argument_default(body, local, location)
        body.add_connected_basic_block("#{local.name}_default")

        exists_reg = local_exists(local, body, location)

        body.instruct(:GotoNextBlockIfTrue, exists_reg, location)

        set_local(local, yield, body, location)
      end

      def on_object(node, body)
        define_object(node, body, Config::OBJECT_CONST)
      end

      def on_trait(node, body)
        define_object(node, body, Config::TRAIT_CONST)
      end

      def redefine_trait(node, body)
        loc = node.location
        trait = get_global(node.name, body, loc)
        block = define_block(
          node.name,
          node.block_type,
          [],
          node.body,
          node.body.locals,
          body,
          loc,
          trait
        )

        run_block(block, [], [], node.type, body, loc)

        trait
      end

      def define_object(node, body, proto_name)
        name = node.name
        loc = node.location
        type = body.self_type.lookup_attribute(name).type
        true_reg = get_true(body, loc)

        object =
          if type.prototype
            proto = get_global(proto_name, body, loc)

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
          loc,
          object
        )

        run_block(block, [], [], node.type, body, loc)

        object
      end

      def on_trait_implementation(node, body)
        loc = node.location
        trait = get_global(node.trait_name.type_name, body, loc)
        object = get_global(node.object_name.name, body, loc)

        implement_trait(object, trait, body, loc)

        block = define_block(
          Config::IMPL_NAME,
          node.block_type,
          [],
          node.body,
          node.body.locals,
          body,
          loc,
          object
        )

        run_block(block, [], [], node.type, body, loc)

        trait
      end

      def implement_trait(object, trait, body, loc)
        name_reg =
          set_string(Config::IMPLEMENTED_TRAITS_INSTANCE_ATTRIBUTE, body, loc)

        traits_reg = body.register(typedb.new_empty_object.new_instance)

        body.add_connected_basic_block

        true_reg = get_true(body, loc)

        body.instruct(:Binary, :GetAttributeInSelf, traits_reg, object, name_reg, loc)
        body.instruct(:GotoNextBlockIfTrue, traits_reg, loc)

        # traits object does not yet exist, create it.
        proto_reg = get_global(Config::OBJECT_CONST, body, loc)

        # create and store the "map" back in the object implementing the trait.
        body.instruct(:SetObject, traits_reg, true_reg, proto_reg, loc)
        body.instruct(:SetAttribute, traits_reg, object, name_reg, traits_reg, loc)

        # register the trait and copy its blocks.
        body.add_connected_basic_block
        body.instruct(:CopyBlocks, object, trait, loc)
        set_attribute(traits_reg, trait, true_reg, body, loc)
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
          loc,
          object
        )

        run_block(block, [], [], node.type, body, loc)

        object
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
        receiver = receiver_for_send(node, body)
        args, kwargs = split_send_arguments(node.arguments, body)

        send_object_message(
          receiver,
          node.name,
          args,
          kwargs,
          node.block_type,
          node.type,
          body,
          node.location
        )
      end

      def split_send_arguments(arguments, body)
        args = []
        kwargs = []

        arguments.each do |arg|
          if arg.keyword_argument?
            kwargs << set_string(arg.name, body, arg.location)
            kwargs << process_node(arg.value, body)
          else
            args << process_node(arg, body)
          end
        end

        [args, kwargs]
      end

      def receiver_for_send(node, body)
        if node.receiver
          process_node(node.receiver, body)
        elsif node.receiver_type == @module.type
          get_global(Config::MODULE_GLOBAL, body, node.location)
        else
          get_self(body, node.location)
        end
      end

      def on_type_cast(node, body)
        process_node(node.expression, body)
      end

      def on_define_variable(node, body)
        callback = node.variable.define_variable_visitor_method
        value = process_node(node.value, body)

        public_send(callback, node.variable, value, body)
      end
      alias on_define_variable_with_explicit_type on_define_variable

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

        get_local_symbol(depth, symbol, body, location)
      end

      def get_local_symbol(depth, symbol, body, location)
        register = body.register(symbol.type)

        if depth >= 0
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

        if symbol.index.negative?
          raise(
            ArgumentError,
            "Global #{name.inspect} does not exist in module #{@module.name}"
          )
        end

        body.instruct(:GetGlobal, register, symbol, location)
      end

      def local_exists(symbol, body, location)
        register = body.register(typedb.boolean_type.new_instance)

        body.instruct(:LocalExists, register, symbol, location)
      end

      def on_define_attribute(node, body)
        # Defining attributes does not generate any code.
      end

      def on_reassign_attribute(variable, value, body)
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

        if depth >= 0
          body.instruct(:SetParentLocal, symbol, depth, value, loc)
        else
          set_local(symbol, value, body, loc)
        end
      end

      def on_raw_instruction(node, body)
        callback = node.raw_instruction_visitor_method

        if respond_to?(callback)
          public_send(callback, node, body)
        else
          get_nil(body, node.location)
        end
      end

      def raw_nullary_instruction(name, node, body)
        reg = body.register(node.type)

        body.instruct(:Nullary, name, reg, node.location)
      end

      def raw_unary_instruction(name, node, body)
        reg = body.register(node.type)
        val = process_node(node.arguments.fetch(0), body)

        body.instruct(:Unary, name, reg, val, node.location)
      end

      def raw_binary_instruction(name, node, body)
        register = body.register(node.type)
        left = process_node(node.arguments.fetch(0), body)
        right = process_node(node.arguments.fetch(1), body)

        body.instruct(:Binary, name, register, left, right, node.location)
      end

      def raw_ternary_instruction(name, node, body)
        register = body.register(node.type)
        one = process_node(node.arguments.fetch(0), body)
        two = process_node(node.arguments.fetch(1), body)
        three = process_node(node.arguments.fetch(2), body)

        body.instruct(:Ternary, name, register, one, two, three, node.location)
      end

      def raw_quaternary_instruction(name, node, body)
        register = body.register(node.type)
        one = process_node(node.arguments.fetch(0), body)
        two = process_node(node.arguments.fetch(1), body)
        three = process_node(node.arguments.fetch(2), body)
        four = process_node(node.arguments.fetch(3), body)

        body.instruct(
          :Quaternary,
          name,
          register,
          one,
          two,
          three,
          four,
          node.location
        )
      end

      def raw_quinary_instruction(name, node, body)
        register = body.register(node.type)
        one = process_node(node.arguments.fetch(0), body)
        two = process_node(node.arguments.fetch(1), body)
        three = process_node(node.arguments.fetch(2), body)
        four = process_node(node.arguments.fetch(3), body)
        five = process_node(node.arguments.fetch(4), body)

        body.instruct(
          :Quinary,
          name,
          register,
          one,
          two,
          three,
          four,
          five,
          node.location
        )
      end

      def builtin_prototype_instruction(id, node, body)
        id_reg = set_integer(id, body, node.location)
        reg = body.register(node.type)

        body.instruct(:Unary, :GetBuiltinPrototype, reg, id_reg, node.location)
      end

      def on_raw_set_prototype(node, body)
        raw_binary_instruction(:SetPrototype, node, body)
      end

      def on_raw_set_attribute(node, body)
        args = node.arguments
        receiver = process_node(args.fetch(0), body)
        name = process_node(args.fetch(1), body)
        value = process_node(args.fetch(2), body)

        set_attribute(receiver, name, value, body, node.location)
      end

      def on_raw_get_attribute(node, body)
        raw_binary_instruction(:GetAttribute, node, body)
      end

      def on_raw_get_attribute_in_self(node, body)
        raw_binary_instruction(:GetAttributeInSelf, node, body)
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

      def on_raw_object_equals(node, body)
        raw_binary_instruction(:ObjectEquals, node, body)
      end

      def on_raw_copy_blocks(node, body)
        to = process_node(node.arguments.fetch(0), body)
        from = process_node(node.arguments.fetch(1), body)

        body.instruct(:CopyBlocks, to, from, node.location)
      end

      def on_raw_integer_to_string(node, body)
        raw_unary_instruction(:IntegerToString, node, body)
      end

      def on_raw_integer_to_float(node, body)
        raw_unary_instruction(:IntegerToFloat, node, body)
      end

      def on_raw_integer_add(node, body)
        raw_binary_instruction(:IntegerAdd, node, body)
      end

      def on_raw_integer_smaller(node, body)
        raw_binary_instruction(:IntegerSmaller, node, body)
      end

      def on_raw_integer_div(node, body)
        raw_binary_instruction(:IntegerDiv, node, body)
      end

      def on_raw_integer_mul(node, body)
        raw_binary_instruction(:IntegerMul, node, body)
      end

      def on_raw_integer_sub(node, body)
        raw_binary_instruction(:IntegerSub, node, body)
      end

      def on_raw_integer_mod(node, body)
        raw_binary_instruction(:IntegerMod, node, body)
      end

      def on_raw_integer_bitwise_and(node, body)
        raw_binary_instruction(:IntegerBitwiseAnd, node, body)
      end

      def on_raw_integer_bitwise_or(node, body)
        raw_binary_instruction(:IntegerBitwiseOr, node, body)
      end

      def on_raw_integer_bitwise_xor(node, body)
        raw_binary_instruction(:IntegerBitwiseXor, node, body)
      end

      def on_raw_integer_shift_left(node, body)
        raw_binary_instruction(:IntegerShiftLeft, node, body)
      end

      def on_raw_integer_shift_right(node, body)
        raw_binary_instruction(:IntegerShiftRight, node, body)
      end

      def on_raw_integer_greater(node, body)
        raw_binary_instruction(:IntegerGreater, node, body)
      end

      def on_raw_integer_equals(node, body)
        raw_binary_instruction(:IntegerEquals, node, body)
      end

      def on_raw_integer_greater_or_equal(node, body)
        raw_binary_instruction(:IntegerGreaterOrEqual, node, body)
      end

      def on_raw_integer_smaller_or_equal(node, body)
        raw_binary_instruction(:IntegerSmallerOrEqual, node, body)
      end

      def on_raw_float_to_string(node, body)
        raw_unary_instruction(:FloatToString, node, body)
      end

      def on_raw_float_to_integer(node, body)
        raw_unary_instruction(:FloatToInteger, node, body)
      end

      def on_raw_float_add(node, body)
        raw_binary_instruction(:FloatAdd, node, body)
      end

      def on_raw_float_div(node, body)
        raw_binary_instruction(:FloatDiv, node, body)
      end

      def on_raw_float_mul(node, body)
        raw_binary_instruction(:FloatMul, node, body)
      end

      def on_raw_float_sub(node, body)
        raw_binary_instruction(:FloatSub, node, body)
      end

      def on_raw_float_mod(node, body)
        raw_binary_instruction(:FloatMod, node, body)
      end

      def on_raw_float_smaller(node, body)
        raw_binary_instruction(:FloatSmaller, node, body)
      end

      def on_raw_float_greater(node, body)
        raw_binary_instruction(:FloatGreater, node, body)
      end

      def on_raw_float_equals(node, body)
        raw_binary_instruction(:FloatEquals, node, body)
      end

      def on_raw_float_greater_or_equal(node, body)
        raw_binary_instruction(:FloatGreaterOrEqual, node, body)
      end

      def on_raw_float_smaller_or_equal(node, body)
        raw_binary_instruction(:FloatSmallerOrEqual, node, body)
      end

      def on_raw_float_is_nan(node, body)
        raw_unary_instruction(:FloatIsNan, node, body)
      end

      def on_raw_float_is_infinite(node, body)
        raw_unary_instruction(:FloatIsInfinite, node, body)
      end

      def on_raw_float_ceil(node, body)
        raw_unary_instruction(:FloatCeil, node, body)
      end

      def on_raw_float_floor(node, body)
        raw_unary_instruction(:FloatFloor, node, body)
      end

      def on_raw_float_round(node, body)
        raw_binary_instruction(:FloatRound, node, body)
      end

      def on_raw_stdout_write(node, body)
        raw_unary_instruction(:StdoutWrite, node, body)
      end

      def on_raw_stdout_flush(node, body)
        raw_nullary_instruction(:StdoutFlush, node, body)
      end

      def on_raw_stderr_flush(node, body)
        raw_nullary_instruction(:StderrFlush, node, body)
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
        args, kwargs = split_send_arguments(node.arguments[1..-1], body)
        return_type = block.type.block? ? block.type.return_type : block.type

        run_block(
          block,
          args,
          kwargs,
          return_type,
          body,
          node.location
        )
      end

      def on_raw_get_string_prototype(node, body)
        builtin_prototype_instruction(PrototypeID::STRING, node, body)
      end

      def on_raw_get_integer_prototype(node, body)
        builtin_prototype_instruction(PrototypeID::INTEGER, node, body)
      end

      def on_raw_get_float_prototype(node, body)
        builtin_prototype_instruction(PrototypeID::FLOAT, node, body)
      end

      def on_raw_get_object_prototype(node, body)
        builtin_prototype_instruction(PrototypeID::OBJECT, node, body)
      end

      def on_raw_get_array_prototype(node, body)
        builtin_prototype_instruction(PrototypeID::ARRAY, node, body)
      end

      def on_raw_get_block_prototype(node, body)
        builtin_prototype_instruction(PrototypeID::BLOCK, node, body)
      end

      def on_raw_array_length(node, body)
        raw_unary_instruction(:ArrayLength, node, body)
      end

      def on_raw_array_at(node, body)
        raw_binary_instruction(:ArrayAt, node, body)
      end

      def on_raw_array_set(node, body)
        register = body.register(node.type)
        array_reg = process_node(node.arguments.fetch(0), body)
        index_reg = process_node(node.arguments.fetch(1), body)
        vreg = process_node(node.arguments.fetch(2), body)
        loc = node.location

        body.instruct(:ArraySet, register, array_reg, index_reg, vreg, loc)
      end

      def on_raw_array_clear(node, body)
        reg = process_node(node.arguments.fetch(0), body)

        body.instruct(:Nullary, :ArrayClear, reg, node.location)
      end

      def on_raw_array_remove(node, body)
        raw_binary_instruction(:ArrayRemove, node, body)
      end

      def on_raw_time_monotonic(node, body)
        raw_nullary_instruction(:TimeMonotonic, node, body)
      end

      def on_raw_time_system(node, body)
        raw_nullary_instruction(:TimeSystem, node, body)
      end

      def on_raw_string_to_upper(node, body)
        raw_unary_instruction(:StringToUpper, node, body)
      end

      def on_raw_string_to_lower(node, body)
        raw_unary_instruction(:StringToLower, node, body)
      end

      def on_raw_string_to_byte_array(node, body)
        raw_unary_instruction(:StringToByteArray, node, body)
      end

      def on_raw_string_size(node, body)
        raw_unary_instruction(:StringSize, node, body)
      end

      def on_raw_string_length(node, body)
        raw_unary_instruction(:StringLength, node, body)
      end

      def on_raw_string_equals(node, body)
        raw_binary_instruction(:StringEquals, node, body)
      end

      def on_raw_string_concat(node, body)
        raw_binary_instruction(:StringConcat, node, body)
      end

      def on_raw_string_slice(node, body)
        raw_ternary_instruction(:StringSlice, node, body)
      end

      def on_raw_string_byte(node, body)
        raw_binary_instruction(:StringByte, node, body)
      end

      def on_raw_stdin_read(node, body)
        raw_binary_instruction(:StdinRead, node, body)
      end

      def on_raw_stderr_write(node, body)
        raw_unary_instruction(:StderrWrite, node, body)
      end

      def on_raw_process_spawn(node, body)
        raw_binary_instruction(:ProcessSpawn, node, body)
      end

      def on_raw_process_send_message(node, body)
        raw_binary_instruction(:ProcessSendMessage, node, body)
      end

      def on_raw_process_receive_message(node, body)
        raw_unary_instruction(:ProcessReceiveMessage, node, body)
      end

      def on_raw_process_current(node, body)
        raw_unary_instruction(:ProcessCurrent, node, body)
      end

      def on_raw_process_suspend_current(node, body)
        timeout = process_node(node.arguments.fetch(0), body)

        body.instruct(:ProcessSuspendCurrent, timeout, node.location)
      end

      def on_raw_process_terminate_current(node, body)
        body.instruct(:ProcessTerminateCurrent, node.location)

        get_nil(body, node.location)
      end

      def on_raw_remove_attribute(node, body)
        raw_binary_instruction(:RemoveAttribute, node, body)
      end

      def on_raw_get_prototype(node, body)
        raw_unary_instruction(:GetPrototype, node, body)
      end

      def on_raw_get_attribute_names(node, body)
        raw_unary_instruction(:GetAttributeNames, node, body)
      end

      def on_raw_attribute_exists(node, body)
        raw_binary_instruction(:AttributeExists, node, body)
      end

      def on_raw_file_flush(node, body)
        file = process_node(node.arguments.fetch(0), body)

        body.instruct(:Nullary, :FileFlush, file, node.location)

        get_nil(body, node.location)
      end

      def on_raw_file_open(node, body)
        raw_ternary_instruction(:FileOpen, node, body)
      end

      def on_raw_file_read(node, body)
        raw_ternary_instruction(:FileRead, node, body)
      end

      def on_raw_file_seek(node, body)
        raw_binary_instruction(:FileSeek, node, body)
      end

      def on_raw_file_size(node, body)
        raw_unary_instruction(:FileSize, node, body)
      end

      def on_raw_file_write(node, body)
        raw_binary_instruction(:FileWrite, node, body)
      end

      def on_raw_file_remove(node, body)
        raw_unary_instruction(:FileRemove, node, body)
      end

      def on_raw_file_copy(node, body)
        raw_binary_instruction(:FileCopy, node, body)
      end

      def on_raw_file_type(node, body)
        raw_unary_instruction(:FileType, node, body)
      end

      def on_raw_file_time(node, body)
        raw_binary_instruction(:FileTime, node, body)
      end

      def on_raw_directory_create(node, body)
        raw_binary_instruction(:DirectoryCreate, node, body)
      end

      def on_raw_directory_remove(node, body)
        raw_binary_instruction(:DirectoryRemove, node, body)
      end

      def on_raw_directory_list(node, body)
        raw_unary_instruction(:DirectoryList, node, body)
      end

      def on_raw_drop(node, body)
        object = process_node(node.arguments.fetch(0), body)

        body.instruct(:Drop, object, node.location)

        get_nil(body, node.location)
      end

      def on_raw_set_blocking(node, body)
        raw_unary_instruction(:SetBlocking, node, body)
      end

      def on_raw_panic(node, body)
        message = process_node(node.arguments.fetch(0), body)

        body.instruct(:Panic, message, node.location)
      end

      def on_raw_exit(node, body)
        status = process_node(node.arguments.fetch(0), body)

        body.instruct(:Exit, status, node.location)
      end

      def on_raw_platform(node, body)
        raw_nullary_instruction(:Platform, node, body)
      end

      def on_raw_hasher_new(node, body)
        raw_ternary_instruction(:HasherNew, node, body)
      end

      def on_raw_hasher_write(node, body)
        raw_binary_instruction(:HasherWrite, node, body)
      end

      def on_raw_hasher_to_hash(node, body)
        raw_unary_instruction(:HasherToHash, node, body)
      end

      def on_raw_hasher_reset(node, body)
        raw_unary_instruction(:HasherReset, node, body)
      end

      def on_raw_stacktrace(node, body)
        raw_binary_instruction(:Stacktrace, node, body)
      end

      def on_raw_block_metadata(node, body)
        raw_binary_instruction(:BlockMetadata, node, body)
      end

      def on_raw_string_format_debug(node, body)
        raw_unary_instruction(:StringFormatDebug, node, body)
      end

      def on_raw_string_concat_multiple(node, body)
        raw_unary_instruction(:StringConcatMultiple, node, body)
      end

      def on_raw_byte_array_from_array(node, body)
        raw_unary_instruction(:ByteArrayFromArray, node, body)
      end

      def on_raw_byte_array_set(node, body)
        raw_ternary_instruction(:ByteArraySet, node, body)
      end

      def on_raw_byte_array_at(node, body)
        raw_binary_instruction(:ByteArrayAt, node, body)
      end

      def on_raw_byte_array_remove(node, body)
        raw_binary_instruction(:ByteArrayRemove, node, body)
      end

      def on_raw_byte_array_length(node, body)
        raw_unary_instruction(:ByteArrayLength, node, body)
      end

      def on_raw_byte_array_clear(node, body)
        reg = process_node(node.arguments.fetch(0), body)

        body.instruct(:Nullary, :ByteArrayClear, reg, node.location)
      end

      def on_raw_byte_array_equals(node, body)
        raw_binary_instruction(:ByteArrayEquals, node, body)
      end

      def on_raw_byte_array_to_string(node, body)
        raw_binary_instruction(:ByteArrayToString, node, body)
      end

      def on_raw_get_boolean_prototype(node, body)
        builtin_prototype_instruction(PrototypeID::BOOLEAN, node, body)
      end

      def on_raw_get_nil_prototype(node, body)
        builtin_prototype_instruction(PrototypeID::NIL, node, body)
      end

      def on_raw_get_module_prototype(node, body)
        builtin_prototype_instruction(PrototypeID::MODULE, node, body)
      end

      def on_raw_get_byte_array_prototype(node, body)
        builtin_prototype_instruction(PrototypeID::BYTE_ARRAY, node, body)
      end

      def on_raw_set_object_name(node, body)
        loc = node.location
        obj = process_node(node.arguments.fetch(0), body)
        val = process_node(node.arguments.fetch(1), body)
        name = set_string(Config::OBJECT_NAME_INSTANCE_ATTRIBUTE, body, loc)

        set_attribute(obj, name, val, body, loc)
      end

      def on_raw_current_file_path(node, body)
        set_current_file_path(body, node.location)
      end

      def on_raw_env_get(node, body)
        raw_unary_instruction(:EnvGet, node, body)
      end

      def on_raw_env_set(node, body)
        raw_binary_instruction(:EnvSet, node, body)
      end

      def on_raw_env_remove(node, body)
        raw_unary_instruction(:EnvRemove, node, body)
      end

      def on_raw_env_variables(node, body)
        raw_nullary_instruction(:EnvVariables, node, body)
      end

      def on_raw_env_home_directory(node, body)
        raw_nullary_instruction(:EnvHomeDirectory, node, body)
      end

      def on_raw_env_temp_directory(node, body)
        raw_nullary_instruction(:EnvTempDirectory, node, body)
      end

      def on_raw_env_get_working_directory(node, body)
        raw_nullary_instruction(:EnvGetWorkingDirectory, node, body)
      end

      def on_raw_env_set_working_directory(node, body)
        raw_unary_instruction(:EnvSetWorkingDirectory, node, body)
      end

      def on_raw_env_arguments(node, body)
        raw_nullary_instruction(:EnvArguments, node, body)
      end

      def on_raw_process_set_panic_handler(node, body)
        raw_unary_instruction(:ProcessSetPanicHandler, node, body)
      end

      def on_raw_process_add_defer_to_caller(node, body)
        raw_unary_instruction(:ProcessAddDeferToCaller, node, body)
      end

      def on_raw_set_default_panic_handler(node, body)
        raw_unary_instruction(:SetDefaultPanicHandler, node, body)
      end

      def on_raw_process_pin_thread(node, body)
        raw_nullary_instruction(:ProcessPinThread, node, body)
      end

      def on_raw_process_unpin_thread(node, body)
        raw_nullary_instruction(:ProcessUnpinThread, node, body)
      end

      def on_raw_process_identifier(node, body)
        raw_unary_instruction(:ProcessIdentifier, node, body)
      end

      def on_raw_library_open(node, body)
        raw_binary_instruction(:LibraryOpen, node, body)
      end

      def on_raw_function_attach(node, body)
        raw_quinary_instruction(:FunctionAttach, node, body)
      end

      def on_raw_function_call(node, body)
        raw_ternary_instruction(:FunctionCall, node, body)
      end

      def on_raw_pointer_attach(node, body)
        raw_ternary_instruction(:PointerAttach, node, body)
      end

      def on_raw_pointer_read(node, body)
        raw_quaternary_instruction(:PointerRead, node, body)
      end

      def on_raw_pointer_write(node, body)
        raw_quaternary_instruction(:PointerWrite, node, body)
      end

      def on_raw_pointer_from_address(node, body)
        raw_binary_instruction(:PointerFromAddress, node, body)
      end

      def on_raw_pointer_address(node, body)
        raw_unary_instruction(:PointerAddress, node, body)
      end

      def on_raw_foreign_type_size(node, body)
        raw_unary_instruction(:ForeignTypeSize, node, body)
      end

      def on_raw_foreign_type_alignment(node, body)
        raw_unary_instruction(:ForeignTypeAlignment, node, body)
      end

      def on_raw_string_to_integer(node, body)
        raw_binary_instruction(:StringToInteger, node, body)
      end

      def on_raw_string_to_float(node, body)
        raw_unary_instruction(:StringToFloat, node, body)
      end

      def on_raw_float_to_bits(node, body)
        raw_unary_instruction(:FloatToBits, node, body)
      end

      def on_raw_socket_create(node, body)
        raw_ternary_instruction(:SocketCreate, node, body)
      end

      def on_raw_socket_write(node, body)
        raw_binary_instruction(:SocketWrite, node, body)
      end

      def on_raw_socket_read(node, body)
        raw_ternary_instruction(:SocketRead, node, body)
      end

      def on_raw_socket_accept(node, body)
        raw_binary_instruction(:SocketAccept, node, body)
      end

      def on_raw_socket_receive_from(node, body)
        raw_ternary_instruction(:SocketReceiveFrom, node, body)
      end

      def on_raw_socket_send_to(node, body)
        raw_quaternary_instruction(:SocketSendTo, node, body)
      end

      def on_raw_socket_address(node, body)
        raw_binary_instruction(:SocketAddress, node, body)
      end

      def on_raw_socket_get_option(node, body)
        raw_binary_instruction(:SocketGetOption, node, body)
      end

      def on_raw_socket_set_option(node, body)
        raw_ternary_instruction(:SocketSetOption, node, body)
      end

      def on_raw_socket_bind(node, body)
        raw_ternary_instruction(:SocketBind, node, body)
      end

      def on_raw_socket_connect(node, body)
        raw_ternary_instruction(:SocketConnect, node, body)
      end

      def on_raw_socket_shutdown(node, body)
        raw_binary_instruction(:SocketShutdown, node, body)
      end

      def on_raw_socket_listen(node, body)
        raw_binary_instruction(:SocketListen, node, body)
      end

      def on_raw_random_number(node, body)
        raw_unary_instruction(:RandomNumber, node, body)
      end

      def on_raw_random_range(node, body)
        raw_binary_instruction(:RandomRange, node, body)
      end

      def on_raw_random_bytes(node, body)
        raw_unary_instruction(:RandomBytes, node, body)
      end

      def on_raw_if(node, body)
        loc = node.location
        rec_node = node.arguments.fetch(0)
        result = body.register(rec_node.type)

        receiver = process_node(rec_node, body)

        body.instruct(:GotoNextBlockIfTrue, receiver, loc)

        # The block used for the "false" argument.
        if_false = process_node(node.arguments.fetch(2), body)

        body.instruct(:Unary, :SetRegister, result, if_false, loc)
        body.instruct(:SkipNextBlock, loc)

        # The block used for the "true" argument.
        body.add_connected_basic_block

        if_true = process_node(node.arguments.fetch(1), body)

        body.instruct(:Unary, :SetRegister, result, if_true, loc)

        body.add_connected_basic_block

        result
      end

      def on_raw_module_load(node, body)
        raw_binary_instruction(:ModuleLoad, node, body)
      end

      def on_raw_module_list(node, body)
        raw_nullary_instruction(:ModuleList, node, body)
      end

      def on_raw_module_get(node, body)
        raw_unary_instruction(:ModuleGet, node, body)
      end

      def on_raw_module_info(node, body)
        raw_binary_instruction(:ModuleInfo, node, body)
      end

      def on_return(node, body)
        location = node.location
        register =
          if node.value
            process_node(node.value, body)
          else
            get_nil(body, location)
          end

        block_return = body.type.closure?

        body.instruct(:Return, block_return, register, location)
        body.add_basic_block
      end

      def on_throw(node, body)
        register = process_node(node.value, body)

        body.instruct(:Nullary, :Throw, register, node.location)

        get_nil(body, node.location)
      end

      def on_try(node, body)
        # A "try" without an "else" block should just re-raise the error.
        unless node.explicit_block_for_else_body?
          return process_node(node.expression, body)
        end

        catch_reg = body.register(body.type.throw_type)
        ret_reg = body.register(node.expression.type)

        # Block for running the to-try expression
        try_block = body.add_connected_basic_block
        try_reg = process_node(node.expression, body)

        body.instruct(:Unary, :SetRegister, ret_reg, try_reg, node.location)
        body.instruct(:SkipNextBlock, node.location)

        # Block for error handling
        else_block = body.add_connected_basic_block
        else_reg = register_for_else_block(node, body, catch_reg)

        body.instruct(:Unary, :SetRegister, ret_reg, else_reg, node.location)

        # Block for everything that comes after our "try" expression.
        body.add_connected_basic_block
        body.catch_table.add_entry(try_block, else_block, catch_reg)

        ret_reg
      end

      def on_dereference(node, body)
        process_node(node.expression, body)
      end

      def register_for_else_block(node, body, catch_reg)
        self_reg = get_self(body, node.else_body.location)
        block_reg = define_block_for_else(node, self_reg, body)
        else_loc = node.else_body.location
        arguments = node.else_argument ? [catch_reg] : []
        return_type = block_reg.type.return_type

        run_block(block_reg, arguments, [], return_type, body, else_loc)
      end

      def define_block_for_else(node, receiver, body)
        loc = node.else_body.location
        block_type = node.else_block_type

        else_code = body.add_code_object(
          block_type.name,
          block_type,
          loc,
          locals: node.else_body.locals
        )

        on_body(node.else_body, else_code)

        block_reg = body.register(block_type)

        body.instruct(:SetBlock, block_reg, else_code, receiver, loc)
      end

      def run_block(block, args, kwargs, return_type, body, location)
        type = block.type
        register = body.register(return_type)

        body.instruct(:RunBlock, register, block, args, kwargs, type, location)
      end

      # Gets and executes a block, without using a fallback.
      #
      # rec - The register containing the receiver a message is sent to.
      # name - The name of the message being sent.
      # args - The arguments passed to the block.
      # kwargs - The keyword arguments passed to the block.
      # block_type - The type of the block being executed.
      # return_type - The type being returned.
      # body - The CompiledCode object to generate the instructions in.
      # loc - The SourceLocation of the operation.
      def run_block_without_unknown_message(
        rec,
        name,
        args,
        kwargs,
        block_type,
        return_type,
        body,
        loc
      )
        block = body.register(block_type)
        name_reg = set_string(name, body, loc)
        register = body.register(return_type)

        body.instruct(:Binary, :GetAttribute, block, rec, name_reg, loc)

        body.instruct(
          :RunBlockWithReceiver,
          register,
          block,
          rec,
          args,
          kwargs,
          block_type,
          loc
        )
      end

      # Gets and executes a block, using a fallback if the block could not be
      # found.
      #
      # rec - The register containing the receiver a message is sent to.
      # name - The name of the message being sent.
      # args - The arguments passed to the block.
      # kwargs - The keyword arguments passed to the block.
      # block_type - The type of the block being executed.
      # return_type - The type being returned.
      # body - The CompiledCode object to generate the instructions in.
      # loc - The SourceLocation of the operation.
      def run_block_with_unknown_message(
        rec,
        name,
        args,
        kwargs,
        block_type,
        return_type,
        body,
        loc
      )
        block_reg = body.register(block_type)
        ret_reg = body.register(return_type)
        args_reg = body.register(
          typedb.new_array_of_type(@module.lookup_any_type.new_instance)
        )

        name_reg = set_string(name, body, loc)
        alt_name_reg = set_string(Config::UNKNOWN_MESSAGE_MESSAGE, body, loc)

        # Look up the block we're supposed to run.
        body.instruct(:Binary, :GetAttribute, block_reg, rec, name_reg, loc)

        # Look up the "unknown_message" block if the initial block was not
        # found.
        goto_block = body.new_basic_block
        body.instruct(:GotoBlockIfTrue, block_reg, goto_block, loc)

        body.instruct(:Binary, :GetAttribute, block_reg, rec, alt_name_reg, loc)

        # Store all the arguments passed in the array and execute the
        # "unknown_message" method.
        body.instruct(:SetArray, args_reg, args, loc)

        body.instruct(
          :RunBlockWithReceiver,
          ret_reg,
          block_reg,
          rec,
          [name_reg, args_reg],
          [],
          block_reg.type,
          loc
        )

        body.instruct(:SkipNextBlock, loc)

        # The code we'd run if the method _is_ defined.
        body.push_connected_basic_block(goto_block)

        body.instruct(
          :RunBlockWithReceiver,
          ret_reg,
          block_reg,
          rec,
          args,
          kwargs,
          block_reg.type,
          loc
        )

        body.add_connected_basic_block

        ret_reg
      end

      def send_to_self(name, block_type, return_type, body, location)
        receiver = get_self(body, location)

        send_object_message(
          receiver,
          name,
          [],
          [],
          block_type,
          return_type,
          body,
          location
        )
      end

      def get_self(body, location)
        get_block_receiver(body, location)
      end

      def get_block_receiver(body, location)
        register = body.register(body.self_type)

        body.instruct(:Nullary, :BlockGetReceiver, register, location)
      end

      def get_nil(body, location)
        register = body.register(typedb.nil_type)

        body.instruct(:Nullary, :GetNil, register, location)
      end

      def get_true(body, location)
        register = body.register(typedb.true_type)

        body.instruct(:Nullary, :GetTrue, register, location)
      end

      def get_false(body, location)
        register = body.register(typedb.false_type)

        body.instruct(:Nullary, :GetFalse, register, location)
      end

      def set_string(value, body, location)
        register = body.register(typedb.string_type)

        body.instruct(:SetLiteral, register, value, location)
      end

      def set_integer(value, body, location)
        register = body.register(typedb.integer_type)

        body.instruct(:SetLiteral, register, value, location)
      end

      def send_object_message(
        rec,
        name,
        arguments,
        kwargs,
        block_type,
        return_type,
        body,
        loc
      )
        rec_type = rec.type

        if send_initializes_array?(rec_type, name)
          send_sets_array(arguments, return_type, body, loc)
        elsif send_runs_block?(rec_type, name)
          run_block(rec, arguments, kwargs, return_type, body, loc)
        else
          lookup_and_run_block(
            rec,
            rec_type,
            name,
            arguments,
            kwargs,
            block_type,
            return_type,
            body,
            loc
          )
        end
      end

      def send_sets_array(arguments, return_type, body, location)
        register = body.register(return_type)

        body.instruct(:SetArray, register, arguments, location)
      end

      def lookup_and_run_block(
        receiver,
        receiver_type,
        name,
        args,
        kwargs,
        block_type,
        return_type,
        body,
        loc
      )
        message =
          if receiver_type.guard_unknown_message?(name)
            :run_block_with_unknown_message
          else
            :run_block_without_unknown_message
          end

        public_send(
          message,
          receiver,
          name,
          args,
          kwargs,
          block_type,
          return_type,
          body,
          loc
        )
      end

      def send_initializes_array?(receiver, name)
        receiver.type_instance_of?(typedb.array_type) &&
          name == Config::NEW_MESSAGE
      end

      def send_runs_block?(receiver, name)
        receiver.block? && name == Config::CALL_MESSAGE
      end

      def get_attribute(receiver, name, body, location)
        rec_type = receiver.type
        symbol = rec_type.lookup_attribute(name)
        name_reg = set_string(name, body, location)
        reg = body.register(symbol.type)

        body.instruct(:Binary, :GetAttribute, reg, receiver, name_reg, location)
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
        prototype = body.register(typedb.object_type)

        body.instruct(:Nullary, :GetObjectPrototype, prototype, location)

        set_object_with_prototype(type, permanent, prototype, body, location)
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
    # rubocop: enable Metrics/ClassLength
  end
end
