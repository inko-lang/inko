# frozen_string_literal: true

module Inkoc
  module Pass
    # rubocop: disable Metrics/ClassLength
    class GenerateTir
      include VisitorMethods

      def initialize(compiler, mod)
        @module = mod
        @state = compiler.state
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

        body.registers.release_all(mod_regs.values)
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
        name_reg = set_string(qname.to_s, body, location)
        reg = body.register(@module.type)
        result = body.instruct(:Unary, :ModuleLoad, reg, name_reg, location)

        body.registers.release(name_reg)

        result
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

        result = set_global(Config::MODULE_GLOBAL, mod_reg, body, loc)

        body.registers.release(mod_reg)
        body.registers.release(mod_name_reg)
        body.registers.release(result)
        nil
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
        result = set_global(import_as, symbol_reg, body, loc)

        body.registers.release(symbol_reg)
        body.registers.release(result)
        nil
      end

      def on_import_self(symbol, source_mod, mod_reg, body)
        return unless symbol.expose?

        import_as = symbol.import_as(source_mod)
        result = set_global(import_as, mod_reg, body, symbol.location)

        body.registers.release(result)
        nil
      end

      def on_import_glob(symbol, source_mod, mod_reg, body)
        loc = symbol.location

        source_mod.attributes.each do |attribute|
          sym_name = attribute.name
          symbol_reg = get_attribute(mod_reg, sym_name, body, loc)
          result = set_global(sym_name, symbol_reg, body, loc)

          body.registers.release(symbol_reg)
          body.registers.release(result)
        end
      end

      def on_body(node, body)
        body.add_connected_basic_block

        node.expressions.each do |expr|
          reg = process_node(expr, body)

          body.registers.release(reg) if reg
        end

        add_explicit_return(body)
        nil
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

        if body.self_type.lookup_attribute(name).any?
          source = get_self(body, loc)
          result = get_attribute(source, name, body, loc)

          body.registers.release(source)

          result
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
        result = define_method(node, receiver, body)

        body.registers.release(receiver)

        result
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
        result = define_block(
          node.block_name,
          node.type,
          node.arguments,
          node.body,
          node.body.locals,
          body,
          node.location,
          this_module
        )

        body.registers.release(this_module)

        result
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

        result =
          set_literal_attribute(receiver, name, block_reg, body, location)

        body.registers.release(block_reg)

        result
      end

      def define_block(
        name,
        type,
        arguments,
        block_body,
        locals,
        body,
        location,
        receiver = TIR::VirtualRegister.reserved
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

          allocate_array(array_reg, [], body, arg.location)
        end
      end

      def generate_argument_default(body, local, location)
        body.add_connected_basic_block("#{local.name}_default")

        exists_reg = local_exists(local, body, location)

        body.instruct(:GotoNextBlockIfTrue, exists_reg, location)

        value = yield
        result = set_local(local, value, body, location)

        body.registers.release(value)

        result
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

        run_block(block, [], node.type, body, loc)

        body.registers.release(block)

        trait
      end

      def define_object(node, body, proto_name)
        name = node.name
        loc = node.location
        type = body.self_type.lookup_attribute(name).type

        proto = get_global(proto_name, body, loc)
        object_reg = body.register(type)
        object = body.instruct(:AllocatePermanent, object_reg, proto, loc)
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

        run_block(block, [], node.type, body, loc)

        body.registers.release(block)
        body.registers.release(proto)

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

        run_block(block, [], node.type, body, loc)

        body.registers.release(object)
        body.registers.release(block)

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
        body.instruct(:AllocatePermanent, traits_reg, proto_reg, loc)
        body.instruct(:SetAttribute, traits_reg, object, name_reg, traits_reg, loc)

        # register the trait and copy its blocks.
        body.add_connected_basic_block
        body.instruct(:CopyBlocks, object, trait, loc)

        result = set_attribute(traits_reg, trait, true_reg, body, loc)

        body.registers.release(name_reg)
        body.registers.release(traits_reg)
        body.registers.release(true_reg)
        body.registers.release(proto_reg)

        result
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

        run_block(block, [], node.type, body, loc)

        body.registers.release(block)

        object
      end

      def set_object_literal_name(object, name, body, location)
        attr = Config::OBJECT_NAME_INSTANCE_ATTRIBUTE
        name_reg = set_string(name, body, location)
        result = set_literal_attribute(object, attr, name_reg, body, location)

        body.registers.release(name_reg)

        result
      end

      def store_object_literal(object, name, body, location)
        receiver = get_self(body, location)
        object =
          set_global_if_module_scope(receiver, name, object, body, location)

        result = set_literal_attribute(receiver, name, object, body, location)

        body.registers.release(receiver)
        body.registers.release(object)

        result
      end

      def on_send(node, body)
        receiver = receiver_for_send(node, body)

        args =
          if node.receiver &&
            send_initializes_array?(node.receiver.type, node.name)
            process_nodes(node.arguments, body)
          else
            send_arguments(node.block_type, node.arguments, body, node.location)
          end

        result = send_object_message(
          receiver,
          node.name,
          args,
          node.block_type,
          node.type,
          body,
          node.location
        )

        body.registers.release(receiver)

        result
      end

      def send_arguments(block, arg_nodes, body, location)
        max_args =
          if block&.block?
            block&.argument_count_without_rest.to_i
          else
            0
          end

        args = Array.new(max_args) { TIR::VirtualRegister.reserved }
        varargs = []

        arg_nodes.each_with_index do |arg, index|
          if arg.keyword_argument? && block
            args[block.arguments[arg.name].index] =
              process_node(arg.value, body)
          elsif index < max_args
            args[index] = process_node(arg, body)
          else
            varargs.push(process_node(arg, body))
          end
        end

        if varargs.any?
          varargs_reg = body.register(
            typedb.new_array_of_type(@module.lookup_any_type.new_instance)
          )

          allocate_array(varargs_reg, varargs, body, location)
          args.push(varargs_reg)
        end

        args
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
        result = public_send(callback, node.variable, value, body)

        body.registers.release(value)

        result
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
          body.registers.release(value)
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
        result = set_literal_attribute(receiver, name, value, body, loc)

        body.registers.release(receiver)
        body.registers.release(value)

        result
      end

      def on_define_constant(variable, value, body)
        loc = variable.location
        name = variable.name
        receiver = get_self(body, loc)
        value = set_global_if_module_scope(receiver, name, value, body, loc)
        result = set_literal_attribute(receiver, name, value, body, loc)

        body.registers.release(receiver)
        body.registers.release(value)

        result
      end

      def on_reassign_variable(node, body)
        callback = node.variable.reassign_variable_visitor_method
        value = process_node(node.value, body)
        result = public_send(callback, node.variable, value, body)

        body.registers.release(value)

        result
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
        result = body.instruct(:Unary, name, reg, val, node.location)

        body.registers.release(val)

        result
      end

      def raw_binary_instruction(name, node, body)
        register = body.register(node.type)
        left = process_node(node.arguments.fetch(0), body)
        right = process_node(node.arguments.fetch(1), body)
        result =
          body.instruct(:Binary, name, register, left, right, node.location)

        body.registers.release(left)
        body.registers.release(right)

        result
      end

      def raw_ternary_instruction(name, node, body)
        register = body.register(node.type)
        one = process_node(node.arguments.fetch(0), body)
        two = process_node(node.arguments.fetch(1), body)
        three = process_node(node.arguments.fetch(2), body)
        result =
          body.instruct(:Ternary, name, register, one, two, three, node.location)

        body.registers.release(one)
        body.registers.release(two)
        body.registers.release(three)

        result
      end

      def raw_quaternary_instruction(name, node, body)
        register = body.register(node.type)
        one = process_node(node.arguments.fetch(0), body)
        two = process_node(node.arguments.fetch(1), body)
        three = process_node(node.arguments.fetch(2), body)
        four = process_node(node.arguments.fetch(3), body)
        result = body.instruct(
          :Quaternary,
          name,
          register,
          one,
          two,
          three,
          four,
          node.location
        )

        body.registers.release(one)
        body.registers.release(two)
        body.registers.release(three)
        body.registers.release(four)

        result
      end

      def raw_quinary_instruction(name, node, body)
        register = body.register(node.type)
        one = process_node(node.arguments.fetch(0), body)
        two = process_node(node.arguments.fetch(1), body)
        three = process_node(node.arguments.fetch(2), body)
        four = process_node(node.arguments.fetch(3), body)
        five = process_node(node.arguments.fetch(4), body)
        result = body.instruct(
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

        body.registers.release(one)
        body.registers.release(two)
        body.registers.release(three)
        body.registers.release(four)
        body.registers.release(five)

        result
      end

      def builtin_prototype_instruction(id, node, body)
        id_reg = set_integer(id, body, node.location)
        reg = body.register(node.type)
        result = body.instruct(:Unary, :GetBuiltinPrototype, reg, id_reg, node.location)

        body.registers.release(id_reg)

        result
      end

      def on_raw_set_attribute(node, body)
        args = node.arguments
        receiver = process_node(args.fetch(0), body)
        name = process_node(args.fetch(1), body)
        value = process_node(args.fetch(2), body)
        result = set_attribute(receiver, name, value, body, node.location)

        body.registers.release(receiver)
        body.registers.release(name)
        body.registers.release(value)

        result
      end

      def on_raw_get_attribute(node, body)
        raw_binary_instruction(:GetAttribute, node, body)
      end

      def on_raw_get_attribute_in_self(node, body)
        raw_binary_instruction(:GetAttributeInSelf, node, body)
      end

      def on_raw_allocate(node, body)
        args = node.arguments
        loc = node.location
        proto = process_node(args.fetch(0), body)
        register = body.register(node.type)

        body.instruct(:Allocate, register, proto, loc)
        body.registers.release(proto)

        register
      end

      def on_raw_allocate_permanent(node, body)
        args = node.arguments
        loc = node.location
        proto = process_node(args.fetch(0), body)
        register = body.register(node.type)
        result = body.instruct(:AllocatePermanent, register, proto, loc)

        body.registers.release(proto)
        body.registers.release(register)

        result
      end

      def on_raw_object_equals(node, body)
        raw_binary_instruction(:ObjectEquals, node, body)
      end

      def on_raw_copy_blocks(node, body)
        to = process_node(node.arguments.fetch(0), body)
        from = process_node(node.arguments.fetch(1), body)
        result = body.instruct(:CopyBlocks, to, from, node.location)

        body.registers.release(to)
        body.registers.release(from)

        result
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
        body.instruct(:Simple, :StdoutFlush, node.location)
        get_nil(body, node.location)
      end

      def on_raw_stderr_flush(node, body)
        body.instruct(:Simple, :StderrFlush, node.location)
        get_nil(body, node.location)
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
        args =
          send_arguments(block.type, node.arguments[1..-1], body, node.location)

        return_type = block.type.block? ? block.type.return_type : block.type

        run_block(
          block,
          args,
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

        body.registers.release(array_reg)
        body.registers.release(index_reg)
        body.registers.release(vreg)

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
        raw_unary_instruction(:ProcessSpawn, node, body)
      end

      def on_raw_process_send_message(node, body)
        raw_binary_instruction(:ProcessSendMessage, node, body)
      end

      def on_raw_process_receive_message(node, body)
        raw_unary_instruction(:ProcessReceiveMessage, node, body)
      end

      def on_raw_process_current(node, body)
        raw_nullary_instruction(:ProcessCurrent, node, body)
      end

      def on_raw_process_suspend_current(node, body)
        timeout = process_node(node.arguments.fetch(0), body)
        result = body.instruct(:ProcessSuspendCurrent, timeout, node.location)

        body.registers.release(timeout)

        result
      end

      def on_raw_process_terminate_current(node, body)
        body.instruct(:ProcessTerminateCurrent, node.location)

        get_nil(body, node.location)
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
        body.registers.release(file)

        get_nil(body, node.location)
      end

      def on_raw_file_open(node, body)
        raw_binary_instruction(:FileOpen, node, body)
      end

      def on_raw_file_path(node, body)
        raw_unary_instruction(:FilePath, node, body)
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

      def on_raw_close(node, body)
        object = process_node(node.arguments.fetch(0), body)

        body.instruct(:Nullary, :Close, object, node.location)
        body.registers.release(object)

        get_nil(body, node.location)
      end

      def on_raw_process_set_blocking(node, body)
        raw_unary_instruction(:ProcessSetBlocking, node, body)
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
        raw_binary_instruction(:HasherNew, node, body)
      end

      def on_raw_hasher_write(node, body)
        raw_binary_instruction(:HasherWrite, node, body)
      end

      def on_raw_hasher_to_hash(node, body)
        raw_unary_instruction(:HasherToHash, node, body)
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

      def on_raw_get_ffi_library_prototype(node, body)
        builtin_prototype_instruction(PrototypeID::FFI_LIBRARY, node, body)
      end

      def on_raw_get_ffi_function_prototype(node, body)
        builtin_prototype_instruction(PrototypeID::FFI_FUNCTION, node, body)
      end

      def on_raw_get_ffi_pointer_prototype(node, body)
        builtin_prototype_instruction(PrototypeID::FFI_POINTER, node, body)
      end

      def on_raw_get_ip_socket_prototype(node, body)
        builtin_prototype_instruction(PrototypeID::IP_SOCKET, node, body)
      end

      def on_raw_get_process_prototype(node, body)
        builtin_prototype_instruction(PrototypeID::PROCESS, node, body)
      end

      def on_raw_get_unix_socket_prototype(node, body)
        builtin_prototype_instruction(PrototypeID::UNIX_SOCKET, node, body)
      end

      def on_raw_get_read_only_file_prototype(node, body)
        builtin_prototype_instruction(PrototypeID::READ_ONLY_FILE, node, body)
      end

      def on_raw_get_write_only_file_prototype(node, body)
        builtin_prototype_instruction(PrototypeID::WRITE_ONLY_FILE, node, body)
      end

      def on_raw_get_read_write_file_prototype(node, body)
        builtin_prototype_instruction(PrototypeID::READ_WRITE_FILE, node, body)
      end

      def on_raw_get_hasher_prototype(node, body)
        builtin_prototype_instruction(PrototypeID::HASHER, node, body)
      end

      def on_raw_get_byte_array_prototype(node, body)
        builtin_prototype_instruction(PrototypeID::BYTE_ARRAY, node, body)
      end

      def on_raw_set_object_name(node, body)
        loc = node.location
        obj = process_node(node.arguments.fetch(0), body)
        val = process_node(node.arguments.fetch(1), body)
        name = set_string(Config::OBJECT_NAME_INSTANCE_ATTRIBUTE, body, loc)
        result = set_attribute(obj, name, val, body, loc)

        body.registers.release(obj)
        body.registers.release(val)
        body.registers.release(name)

        result
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

      def on_raw_process_set_pinned(node, body)
        raw_unary_instruction(:ProcessSetPinned, node, body)
      end

      def on_raw_process_identifier(node, body)
        raw_unary_instruction(:ProcessIdentifier, node, body)
      end

      def on_raw_ffi_library_open(node, body)
        raw_unary_instruction(:FFILibraryOpen, node, body)
      end

      def on_raw_ffi_function_attach(node, body)
        raw_quaternary_instruction(:FFIFunctionAttach, node, body)
      end

      def on_raw_ffi_function_call(node, body)
        raw_binary_instruction(:FFIFunctionCall, node, body)
      end

      def on_raw_ffi_pointer_attach(node, body)
        raw_binary_instruction(:FFIPointerAttach, node, body)
      end

      def on_raw_ffi_pointer_read(node, body)
        raw_ternary_instruction(:FFIPointerRead, node, body)
      end

      def on_raw_ffi_pointer_write(node, body)
        raw_quaternary_instruction(:FFIPointerWrite, node, body)
      end

      def on_raw_ffi_pointer_from_address(node, body)
        raw_unary_instruction(:FFIPointerFromAddress, node, body)
      end

      def on_raw_ffi_pointer_address(node, body)
        raw_unary_instruction(:FFIPointerAddress, node, body)
      end

      def on_raw_ffi_type_size(node, body)
        raw_unary_instruction(:FFITypeSize, node, body)
      end

      def on_raw_ffi_type_alignment(node, body)
        raw_unary_instruction(:FFITypeAlignment, node, body)
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
        raw_binary_instruction(:SocketCreate, node, body)
      end

      def on_raw_socket_write(node, body)
        raw_binary_instruction(:SocketWrite, node, body)
      end

      def on_raw_socket_read(node, body)
        raw_ternary_instruction(:SocketRead, node, body)
      end

      def on_raw_socket_accept(node, body)
        raw_unary_instruction(:SocketAccept, node, body)
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

        body.instruct(:Unary, :CopyRegister, result, if_false, loc)
        body.instruct(:SkipNextBlock, loc)

        # The block used for the "true" argument.
        body.add_connected_basic_block

        if_true = process_node(node.arguments.fetch(1), body)

        body.instruct(:Unary, :CopyRegister, result, if_true, loc)

        body.add_connected_basic_block
        body.registers.release(receiver)
        body.registers.release(if_false)
        body.registers.release(if_true)

        result
      end

      def on_raw_module_load(node, body)
        raw_unary_instruction(:ModuleLoad, node, body)
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

        method_return = body.type.closure?

        body.instruct(:Return, method_return, register, location)
      end

      def on_throw(node, body)
        register = process_node(node.value, body)

        body.instruct(:Nullary, :Throw, register, node.location)
        body.registers.release(register)

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

        body.instruct(:Unary, :CopyRegister, ret_reg, try_reg, node.location)
        body.instruct(:SkipNextBlock, node.location)

        # Block for error handling
        else_block = body.add_connected_basic_block

        body.instruct(:Nullary, :MoveResult, catch_reg, node.location)

        else_reg = register_for_else_block(node, body, catch_reg)

        body.instruct(:Unary, :CopyRegister, ret_reg, else_reg, node.location)

        # Block for everything that comes after our "try" expression.
        body.add_connected_basic_block
        body.catch_table.add_entry(try_block, else_block)

        ret_reg
      end

      def on_dereference(node, body)
        process_node(node.expression, body)
      end

      def on_coalesce_nil(node, body)
        expr = process_node(node.expression, body)

        body.instruct(:GotoNextBlockIfTrue, expr, node.location)

        default = process_node(node.default, body)

        body.instruct(:Unary, :CopyRegister, expr, default, node.location)
        body.add_connected_basic_block
        body.registers.release(default)

        expr
      end

      def on_new_instance(node, body)
        proto_name =
          if node.self_type?
            node.type.base_type.name
          else
            node.name
          end

        proto = get_global(proto_name, body, node.location)
        instance = body.register(node.type)

        body.instruct(:Allocate, instance, proto, node.location)
        body.registers.release(proto)

        node.attributes.each do |attr|
          name = set_string(attr.name, body, attr.location)
          value = process_node(attr.value, body)

          set_attribute(instance, name, value, body, attr.location)

          body.registers.release(name)
          body.registers.release(value)
        end

        # This hack is necessary because so the last instruction is the one that
        # produces a value we can return. Without this, methods such as
        # add_explicit_return() would operate on the last set attribute, not the
        # newly created instance.
        body.instruct(:Unary, :CopyRegister, instance, instance, node.location)
      end

      def register_for_else_block(node, body, catch_reg)
        self_reg = get_self(body, node.else_body.location)
        block_reg = define_block_for_else(node, self_reg, body)
        else_loc = node.else_body.location
        arguments = node.else_argument ? [catch_reg] : []
        return_type = block_reg.type.return_type

        run_block(block_reg, arguments, return_type, body, else_loc)
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

      def run_block(block, args, return_type, body, location)
        type = block.type
        register = body.register(return_type)
        args = make_registers_contiguous(args, body, location)
        start = args.first || register

        body.registers.release_all(args)
        body.instruct(:RunBlock, block, start, args.length, type, location)
        body.instruct(:Nullary, :MoveResult, register, location)
      end

      # Gets and executes a block, without using a fallback.
      #
      # rec - The register containing the receiver a message is sent to.
      # name - The name of the message being sent.
      # args - The arguments passed to the block.
      # block_type - The type of the block being executed.
      # return_type - The type being returned.
      # body - The CompiledCode object to generate the instructions in.
      # loc - The SourceLocation of the operation.
      def run_block_without_unknown_message(
        rec,
        name,
        args,
        block_type,
        return_type,
        body,
        loc
      )
        block = body.register(block_type)
        name_reg = set_string(name, body, loc)
        register = body.register(return_type)
        args = make_registers_contiguous(args, body, loc)
        start = args.first || register

        body.instruct(:Binary, :GetAttribute, block, rec, name_reg, loc)

        body.instruct(
          :RunBlockWithReceiver,
          block,
          rec,
          start,
          args.length,
          block_type,
          loc
        )

        body.instruct(:Nullary, :MoveResult, register, loc)
      end

      # Gets and executes a block, using a fallback if the block could not be
      # found.
      #
      # rec - The register containing the receiver a message is sent to.
      # name - The name of the message being sent.
      # args - The arguments passed to the block.
      # block_type - The type of the block being executed.
      # return_type - The type being returned.
      # body - The CompiledCode object to generate the instructions in.
      # loc - The SourceLocation of the operation.
      def run_block_with_unknown_message(
        rec,
        name,
        args,
        block_type,
        return_type,
        body,
        loc
      )
        ret_reg = body.register(return_type)
        block_reg = body.register(block_type)

        args = make_registers_contiguous(args, body, loc)
        start_arg = args.first || ret_reg

        # Re-ordering these two instructions will break the RunBlockWithReceiver
        # below, so don't.
        name_reg = set_string(name, body, loc)
        args_reg = body.register(
          typedb.new_array_of_type(@module.lookup_any_type.new_instance)
        )

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
        allocate_array(args_reg, args, body, loc)

        body.instruct(
          :RunBlockWithReceiver,
          block_reg,
          rec,
          name_reg,
          2,
          block_reg.type,
          loc
        )

        body.instruct(:Nullary, :MoveResult, ret_reg, loc)
        body.instruct(:SkipNextBlock, loc)

        # The code we'd run if the method _is_ defined.
        body.push_connected_basic_block(goto_block)

        body.instruct(
          :RunBlockWithReceiver,
          block_reg,
          rec,
          start_arg,
          args.length,
          block_reg.type,
          loc
        )

        body.instruct(:Nullary, :MoveResult, ret_reg, loc)
        body.add_connected_basic_block

        ret_reg
      end

      def send_to_self(name, block_type, return_type, body, location)
        receiver = get_self(body, location)

        send_object_message(
          receiver,
          name,
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
        block_type,
        return_type,
        body,
        loc
      )
        rec_type = rec.type

        if send_initializes_array?(rec_type, name)
          send_sets_array(arguments, return_type, body, loc)
        elsif send_runs_block?(rec_type, name)
          run_block(rec, arguments, return_type, body, loc)
        else
          lookup_and_run_block(
            rec,
            rec_type,
            name,
            arguments,
            block_type,
            return_type,
            body,
            loc
          )
        end
      end

      def send_sets_array(arguments, return_type, body, location)
        register = body.register(return_type)

        allocate_array(register, arguments, body, location)
      end

      def lookup_and_run_block(
        receiver,
        receiver_type,
        name,
        args,
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

        body.registers.release(name_reg)

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

      def allocate_array(register, values, body, location)
        length = values.length
        args = make_registers_contiguous(values, body, location)
        start = args.first || register

        body.registers.release_all(args)
        body.instruct(:ArrayAllocate, register, start, length, location)
      end

      def make_registers_contiguous(registers, body, location)
        return registers if registers.empty?

        last_id = registers.first.id.to_i
        move = false

        registers.each do |reg|
          # When padding arguments, the register for argument A may be greater
          # than the register for argument B. For example, the registers for two
          # arguments may be [69, 68].
          if reg.id < last_id || (reg.id - last_id) > 1
            move = true
            break
          end

          if registers.length > 1 && last_id.zero?
            move = true
            break
          end

          last_id = reg.id
        end

        if move
          new_registers = body.registers.allocate_range(registers.map(&:type))

          registers.zip(new_registers) do |old_reg, new_reg|
            body.instruct(:Unary, :CopyRegister, new_reg, old_reg, location)
          end

          new_registers
        else
          registers
        end
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
