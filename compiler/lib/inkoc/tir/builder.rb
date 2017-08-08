# frozen_string_literal: true

module Inkoc
  module TIR
    class Builder
      def initialize(state)
        @state = state
      end

      # Builds the main module.
      def build_main(path)
        qname = QualifiedName.new([module_name_for_path(path)])
        name = qname.to_s

        @state.track_module_before_compilation(name)

        mod = build(qname, path)

        @state.store_module(name, mod)
      end

      # Builds a single module and returns it.
      #
      # qname - The QualifiedName of the module.
      # path - The file path to the module.
      def build(qname, path)
        ast = parse_file(path)

        return unless ast

        location = SourceLocation.first_line(SourceFile.new(path))
        mod = Module.new(qname, location)

        module_body(ast, mod)

        mod
      end

      def compile_module(qname, path, location)
        name = qname.to_s

        return if @state.module_compiled?(name)

        # We insert the module name before processing it to prevent the
        # compiler from getting stuck in a recursive import.
        @state.track_module_before_compilation(name)

        if (full_path = find_module_path(path))
          @state.store_module(name, build(qname, full_path))
        else
          diagnostics.module_not_found_error(qname.to_s, location)
          nil
        end
      end

      # Builds the body of a module.
      def module_body(ast, mod)
        mod.body.add_connected_basic_block('module_prelude')

        import_bootstrap_module(mod)
        import_prelude_module(mod)
        define_module_object(mod)

        on_body(ast, mod.body, mod)
      end

      def define_module_object(mod)
        self_local = mod.body.define_self_local(mod.type)
        body = mod.body
        location = mod.location
        qname_array = array_of_strings(mod.name.parts, body, location)

        def_mod = send_object_message(
          get_toplevel(body, location),
          Config::DEFINE_MODULE_MESSAGE,
          [qname_array],
          body,
          location
        )

        set_local(self_local, def_mod, body, location)
      end

      def import_bootstrap_module(mod)
        register = mod.body.register(dynamic_type)
        path = set_string(Config::BOOTSTRAP_FILE, mod.body, mod.location)

        mod.body.instruct(:LoadModule, register, path, mod.location)

        register
      end

      def import_prelude_module(mod)
        # TODO: implement prelude importing
      end

      def process_nodes(nodes, body, mod)
        nodes.map { |node| process_node(node, body, mod) }
      end

      def process_node(node, body, mod)
        public_send(node.tir_process_node_method, node, body, mod)
      end

      def on_body(node, body, mod)
        body.add_connected_basic_block

        registers = process_nodes(node.expressions, body, mod)

        add_explicit_return(body)
        check_for_unreachable_blocks(body)

        registers
      end

      def on_integer(node, body, _)
        type = @state.typedb.integer_type

        set_literal(:SetInteger, type, node.value, body, node.location)
      end

      def on_float(node, body, _)
        type = @state.typedb.float_type

        set_literal(:SetFloat, type, node.value, body, node.location)
      end

      def on_string(node, body, _)
        set_string(node.value, body, node.location)
      end

      def on_array(node, body, mod)
        values = process_nodes(node.values, body, mod)
        type = Type::Array.new(@state.typedb.array_prototype)

        # TODO: generic array type + validation

        set_literal(:SetArray, type, values, body, node.location)
      end

      def on_hash_map(node, body, mod)
        pairs = node.pairs.map do |(key, value)|
          [process_node(key, body, mod), process_node(value, body, mod)]
        end

        type = dynamic_type # TODO: proper hash type

        set_literal(:SetHashMap, type, pairs, body, node.location)
      end

      def on_self(node, body, _)
        get_self(body, node.location)
      end

      def on_identifier(node, body, mod)
        name = node.name
        loc = node.location

        if body.locals.defined?(name)
          get_local(body.locals[name], body, loc)
        elsif mod.globals.defined?(name)
          get_global(mod.globals[name], body, loc)
        else
          send_to_self(name, [], body, loc)
        end
      end

      def on_attribute(node, body, _)
        name = node.name
        location = node.location
        receiver = get_self(body, node.location)

        unless receiver.type.attribute?(name)
          diagnostics.undefined_attribute_error(name, location)
        end

        get_attribute(receiver, name, body, location)
      end

      def on_constant(node, body, mod)
        name = node.name
        location = node.location

        if node.receiver
          receiver = process_node(node.receiver, body, mod)

          unless receiver.type.attribute?(name)
            diagnostics.undefined_constant_error(name, location)
          end

          get_attribute(receiver, name, body, location)
        else
          symbol = mod.globals[name]

          diagnostics.undefined_constant_error(name, location) if symbol.nil?

          get_global(symbol, body, location)
        end
      end

      def on_define_variable(node, body, mod)
        value = process_node(node.value, body, mod)
        method = node.variable.tir_define_variable_method

        if node.value_type
          # TODO: ensure the tagged and value types match
        end

        public_send(method, node, value, body, mod)
      end

      def on_define_local(node, value, body, _)
        name = node.variable.name
        location = node.location

        if body.locals.defined?(name)
          diagnostics.redefine_existing_local_error(name, location)
        end

        local = body.locals.define(name, value.type, node.mutable?)

        set_local(local, value, body, location)
      end

      def on_define_constant(node, value, body, mod)
        location = node.location
        name = node.variable.name
        receiver = get_self(body, location)

        diagnostics.mutable_constant_error(location) if node.mutable?

        if receiver.type.attribute?(name)
          diagnostics.redefine_existing_constant_error(name, location)
        end

        value_reg = set_attribute(receiver, name, value, body, location)

        # Constants defined at the top-level should also be available as module
        # globals.
        if module_scope?(body, mod)
          define_global(name, value_reg, body, location, mod)
        else
          value_reg
        end
      end

      def on_define_attribute(node, value, body, _)
        name = node.variable.name
        location = node.location
        rec_reg = get_self(body, location)
        rec_type = rec_reg.type

        if rec_type.lookup_attribute(name).any?
          diagnostics.redefine_existing_attribute_error(name, location)
        else
          rec_type.define_attribute(name, value.type, node.mutable?)
        end

        set_attribute(rec_reg, name, value, body, location)
      end

      def on_reassign_variable(node, body, mod)
        method = node.variable.tir_reassign_variable_method

        public_send(method, node, body, mod)
      end

      def on_reassign_local(node, body, mod)
        name = node.variable.name
        location = node.location
        local = body.locals[name]

        diagnostics.undefined_local_error(name, location) if local.nil?

        # TODO: verify the type of the new value
        value = process_node(node.value, body, mod)

        set_local(local, value, body, location)

        value
      end

      def on_reassign_attribute(node, body, mod)
        name = node.variable.name
        location = node.location
        receiver = get_self(body, location)

        if receiver.type.lookup_attribute(name).nil?
          diagnostics.undefined_attribute_error(name, location)
        end

        # TODO: verify the type of the new value
        value = process_node(node.value, body, mod)

        set_attribute(receiver, name, value, body, location)
      end

      def on_send(node, body, mod)
        name = node.name
        location = node.location
        receiver = if node.receiver
                     process_node(node.receiver, body, mod)
                   else
                     get_self(body, location)
                   end

        arguments = node.arguments.map do |arg|
          process_node(arg, body, mod)
        end

        send_object_message(receiver, name, arguments, body, location)
      end

      def on_import(node, body, mod)
        qname = node.qualified_name
        mod_name = qname.module_name
        mod_path = qname.source_path_with_extension
        location = node.location

        compile_module(qname, mod_path, location)

        loaded_mod = send_object_message(
          get_toplevel(body, location),
          Config::LOAD_MODULE_MESSAGE,
          [array_of_strings(qname.parts, body, location)],
          body,
          location
        )

        if node.symbols.empty?
          # If no symbols are given we'll import the module itself as a global
          # using the same name.
          import_without_symbols(loaded_mod, mod_name, body, location, mod)
        else
          import_with_symbols(loaded_mod, node.symbols, body, location, mod)
        end

        loaded_mod
      end

      def import_without_symbols(mod_reg, mod_name, body, location, mod)
        global = mod.globals.define(mod_name, dynamic_type)

        set_global(global, mod_reg, body, location)
      end

      def import_with_symbols(mod_reg, symbols, body, location, mod)
        symbols.each do |symbol|
          symbol_reg = send_object_message(
            mod_reg,
            Config::SYMBOL_MESSAGE,
            [set_string(symbol.symbol_name, body, location)],
            body,
            location
          )

          global = mod.globals.define(symbol.import_as, dynamic_type)

          set_global(global, symbol_reg, body, location)
        end
      end

      def on_block(node, body, mod)
        location = node.location

        block_code, type =
          new_block_from_block_ast('<body>', node, body, location, mod)

        set_block(block_code, type, body, location)
      end

      def set_block(code_object, type, body, location)
        register = body.register(type)

        body.instruct(:SetBlock, register, code_object, location)

        register
      end

      def new_block_from_block_ast(name, node, body, location, mod)
        new_block(
          name,
          node.body,
          node.arguments,
          type_of_self(body),
          body,
          location,
          mod,
          throws: node.throws,
          returns: node.returns
        )
      end

      # Creates a new block and returns the CodeObject and type object of the
      # block.
      #
      # name - The name of the block's CodeObject.
      # block_body - An instance of AST::Body containing.
      # block_args - An Array of AST::DefineArgument instances.
      # self_type - The type of "self" in the block.
      # body - The CodeObject to use for generating instructions.
      # location - The SourceLocation to use for the instructions.
      # mod - The Module being compiled.
      # throws - The AST node containing the type being thrown, if any.
      # returns - The AST node containing the type returned, if any.
      def new_block(
        name,
        block_body,
        block_args,
        self_type,
        body,
        location,
        mod,
        throws: nil,
        returns: nil
      )
        block_code = body.add_code_object(name, location)
        type = Type::Block.new(@state.typedb.block_prototype)

        define_arguments_for_block_type(block_args, self_type, type, mod)
        define_block_arguments(block_args, block_code, self_type, type, mod)
        define_throw_type(throws, self_type, type, mod)
        define_return_type(returns, self_type, type, mod)

        on_body(block_body, block_code, mod)

        [block_code, type]
      end

      def define_arguments_for_block_type(arguments, self_type, type, mod)
        type.argument_types.insert(0, self_type)

        arguments.each do |arg|
          # TODO: infer type based on default value, if any
          arg_type = if arg.type
                       type_for_constant_node(arg.type, type, self_type, mod)
                     else
                       dynamic_type
                     end

          type.argument_types << arg_type
          type.rest_argument = true if arg.rest?
        end
      end

      def define_block_arguments(arguments, body, self_type, type, mod)
        body.define_self_local(self_type)

        arguments.each_with_index do |arg, index|
          arg_type = type.argument_types[index]
          local = body.locals.define(arg.name, arg_type)

          define_argument_default(arg, local, body, mod) if arg.default
        end
      end

      def define_throw_type(node, self_type, type, mod)
        return unless node

        type.throws = type_for_constant_node(node, self_type, type, mod)
      end

      def define_return_type(node, self_type, type, mod)
        type.returns =
          if node
            type_for_constant_node(node, self_type, type, mod)
          else
            dynamic_type
          end
      end

      # Generates the instructions necessary to set the default value of a block
      # argument.
      #
      # This method will return the BasicBlock containing the instructions used
      # for setting the argument's default value.
      #
      # arg - An intance of AST::DefineArgument to process.
      # local - The Inkoc::Symbol associated with the argument.
      # body - The TIR::CodeObject to use for generating instructions
      # mod - The TIR::Module that is being compiled.
      def define_argument_default(arg, local, body, mod)
        body.add_connected_basic_block("#{arg.name}_default")

        location = arg.default.location
        exists_reg = body.register(@state.typedb.boolean_type)

        body.instruct(:LocalExists, exists_reg, local, location)
        body.instruct(:GotoNextBlockIfTrue, exists_reg, location)

        value_reg = process_node(arg.default, body, mod)

        body.instruct(:SetLocal, local, value_reg, location)
      end

      def on_return(node, body, mod)
        register = process_node(node.value, body, mod)

        return_value(register, body, node.location)

        body.add_basic_block

        register
      end

      def add_explicit_return(body)
        ins = body.current_block.instructions.last

        if ins && !ins.is_a?(Instruction::Return)
          return_value(ins.register, body, ins.location)
        elsif !ins
          location = body.location

          return_value(get_nil(body, location), body, location)
        end
      end

      def check_for_unreachable_blocks(body)
        body.blocks.each do |block|
          next if body.reachable_basic_block?(block)

          diagnostics.unreachable_code_warning(block.location)
        end
      end

      def return_value(value, body, location)
        body.instruct(:Return, value, location)

        value
      end

      def on_method(node, body, mod)
        location = node.location
        receiver = get_self(body, location)

        if node.required?
          define_required_method(node, receiver, location, mod)
        else
          define_method(node, receiver, body, location, mod)
        end
      end

      def define_method(node, receiver, body, location, mod)
        type_name = "#{receiver.type.name}.#{node.name}"

        block_code, type =
          new_block_from_block_ast(type_name, node, body, location, mod)

        block_reg = set_block(block_code, type, body, location)

        set_attribute(receiver, node.name, block_reg, body, location)
      end

      def define_required_method(node, receiver, location, mod)
        name = node.name
        type = receiver.type

        unless type.trait?
          diagnostics.define_required_method_on_non_trait_error(location)
          return
        end

        block_type = Type::Block.new(@state.typedb.block_prototype)

        define_arguments_for_block_type(
          node.arguments,
          receiver.type,
          block_type,
          mod
        )

        define_throw_type(node.throws, type, block_type, mod)
        define_return_type(node.returns, type, block_type, mod)

        type.required_methods.define(name, block_type)
      end

      # Compiles a named object definition.
      def on_object(node, body, mod)
        receiver = get_self(body, node.location)
        symbol = receiver.type.lookup_attribute(node.name)

        if symbol.any?
          unless symbol.type.regular_object?
            diagnostics.reopen_invalid_object_error(node.name, node.location)
          end

          reopen_existing_object(node, receiver, body, mod)
        else
          define_new_object(node, receiver, body, mod)
        end
      end

      def define_new_object(node, receiver, body, mod)
        location = node.location
        name = node.name
        object = new_named_object(name, body, location, mod)

        # TODO: process trait implementations
        define_type_arguments(node.type_arguments, object.type, mod)

        if module_scope?(body, mod)
          object = define_global(name, object, body, location, mod)
        end

        set_attribute(receiver, name, object, body, location)

        evaluate_object_body(node, object, body, mod)
      end

      def reopen_existing_object(node, receiver, body, mod)
        location = node.location
        name = node.name
        object = get_attribute(receiver, name, body, location)

        evaluate_object_body(node, object, body, mod)
      end

      def new_named_object(name, body, location, mod)
        prototype =
          get_global(mod.globals[Config::OBJECT_CONST], body, location)

        global = get_true(body, location)

        set_object(global, prototype, body, location, name: name)
      end

      def evaluate_object_body(node, object_reg, body, mod)
        location = node.location

        # Create a block for the object's body and execute it, with "self" set
        # to the object itself.
        block_code, block_type = new_block(
          node.name,
          node.body,
          [],
          object_reg.type,
          body,
          location,
          mod
        )

        block_reg = set_block(block_code, block_type, body, location)
        ret_reg = body.register(dynamic_type)

        body.instruct(:RunBlock, ret_reg, block_reg, [object_reg], location)

        object_reg
      end

      def set_object(
        global,
        prototype,
        body,
        location,
        name: nil,
        type_class: Type::Object
      )
        type = type_class.new(name, prototype.type)
        register = body.register(type)

        body.instruct(:SetObject, register, global, prototype, location)

        register
      end

      def on_trait(node, body, mod)
        receiver = get_self(body, node.location)

        if receiver.type.attribute?(node.name)
          diagnostics.redefine_trait_error(node.location)
        else
          define_new_trait(node, receiver, body, mod)
        end
      end

      def define_new_trait(node, receiver, body, mod)
        location = node.location
        name = node.name
        trait = new_trait(name, body, location, mod)

        define_type_arguments(node.type_arguments, trait.type, mod)

        if module_scope?(body, mod)
          trait = define_global(name, trait, body, location, mod)
        end

        set_attribute(receiver, name, trait, body, location)

        evaluate_object_body(node, trait, body, mod)
      end

      def new_trait(name, body, location, mod)
        prototype =
          get_global(mod.globals[Config::TRAIT_CONST], body, location)

        global = get_true(body, location)

        set_object(
          global,
          prototype,
          body,
          location,
          name: name,
          type_class: Type::Trait
        )
      end

      def define_type_arguments(arguments, type, mod)
        arguments.each do |arg_node|
          required_traits = arg_node.required_traits.map do |node|
            type_for_constant_node(node, type, mod)
          end

          arg = Type::TypeArgument.new(arg_node.name, required_traits)

          type.type_arguments.define(arg.name, arg)
        end
      end

      def get_nil(body, location)
        register = body.register(@state.typedb.nil_type)

        body.instruct(:GetNil, register, location)

        register
      end

      def get_true(body, location)
        register = body.register(@state.typedb.boolean_type)

        body.instruct(:GetTrue, register, location)

        register
      end

      def get_false(body, location)
        register = body.register(@state.typedb.boolean_type)

        body.instruct(:GetFalse, register, location)

        register
      end

      # Gets an attribute from a register.
      #
      # receiver - The register to get the attribute from.
      # name - The name of the attribute as a String.
      def get_attribute(
        receiver,
        name,
        body,
        location
      )
        rec_type = receiver.type
        attribute = rec_type.lookup_attribute(name)
        register = body.register(attribute.type)
        name_reg = set_string(name, body, location)

        body.instruct(:GetAttribute, register, receiver, name_reg, location)

        register
      end

      # Sets an attribute in an object.
      #
      # receiver - The register containing the object to store the value in.
      # name - The name of the attribute to set as a String
      # value - The register containing the value to set.
      def set_attribute(receiver, name, value, body, location)
        register = body.register(value.type)
        name_reg = set_string(name, body, location)

        receiver.type.define_attribute(name, value.type)

        body.instruct(
          :SetAttribute,
          register,
          receiver,
          name_reg,
          value,
          location
        )

        register
      end

      # Sends a message to "self"
      #
      # name - The name of the message as a String.
      # arguments - The arguments to pass as an Array of VirtualRegister
      #             objects.
      # body - The CodeObject to store the instructions in.
      # location - The SourceLocation for the instruction.
      def send_to_self(name, arguments, body, location)
        receiver = get_self(body, location)

        send_object_message(receiver, name, arguments, body, location)
      end

      # Sends a message to an object.
      #
      # receiver - The receiver of the message as a VirtualRegister.
      # name - The name of the message as a String.
      # arguments - The arguments to pass as an Array of VirtualRegister
      #             objects.
      # body - The CodeObject to store the instruction in.
      # location - The SourceLocation for the instruction.
      def send_object_message(receiver, name, arguments, body, location)
        rec_type = receiver.type
        reg_type = rec_type.message_return_type(name)
        register = body.register(reg_type)
        name_reg = set_string(name, body, location)

        unless rec_type.responds_to_message?(name)
          diagnostics.undefined_method_error(name, location)
        end

        body.instruct(
          :SendObjectMessage,
          register,
          receiver,
          name_reg,
          arguments,
          location
        )

        register
      end

      def get_self(body, location)
        symbol = body.locals[Config::SELF_LOCAL]

        get_local(symbol, body, location)
      end

      def get_toplevel(body, location)
        register = body.register(Type::Object.new(@state.typedb.top_level))

        body.instruct(:GetToplevel, register, location)

        register
      end

      def get_local(symbol, body, location)
        register = body.register(symbol.type)

        body.instruct(:GetLocal, register, symbol, location)

        register
      end

      def get_global(symbol, body, location)
        register = body.register(symbol.type)

        body.instruct(:GetGlobal, register, symbol, location)

        register
      end

      def set_global(symbol, value, body, location)
        register = body.register(value.type)

        body.instruct(:SetGlobal, register, symbol, value, location)

        register
      end

      def define_global(name, value, body, location, mod)
        symbol = mod.globals.define(name, value.type)

        set_global(symbol, value, body, location)
      end

      def set_string(value, body, location)
        type = @state.typedb.string_type

        set_literal(:SetString, type, value, body, location)
      end

      def set_literal(instruction, type, value, body, location)
        register = body.register(type)

        body.instruct(instruction, register, value, location)

        register
      end

      def set_local(local_symbol, value, body, location)
        body.instruct(:SetLocal, local_symbol, value, location)

        value
      end

      # Sets an array of strings in a register.
      #
      # values - An Array of Strings to store in the array.
      # body - The CodeObject to store the instructions in.
      # location - The SourceLocation for the instruction.
      def array_of_strings(values, body, location)
        type = Type::Array.new(@state.typedb.array_prototype)
        register = body.register(type)
        value_regs = values.map { |value| set_string(value, body, location) }

        body.instruct(:SetArray, register, value_regs, location)

        register
      end

      def type_for_constant_node(node, *sources)
        name = node.name

        if node.receiver
          receiver = type_for_constant_node(node.receiver, *sources)
          sources = [receiver, *sources]
        end

        sources.each do |source|
          symbol = source.lookup_type(name)

          return symbol.type if symbol.any?
        end

        diagnostics.undefined_constant_error(node.name, node.location)

        dynamic_type
      end

      # Returns the type of "self" in the given CodeObject.
      def type_of_self(body)
        body.locals[Config::SELF_LOCAL].type
      end

      def module_scope?(body, mod)
        body == mod.body
      end

      def find_module_path(path)
        @state.config.source_directories.each do |dir|
          full_path = File.join(dir, path)

          return full_path if File.file?(full_path)
        end

        nil
      end

      # Returns the module name for a file path.
      #
      # Example:
      #
      #     module_name_for_path('hello/world.inko') # => "world"
      def module_name_for_path(path)
        file = path.split(File::SEPARATOR).last

        file ? file.split('.').first : '<anonymous-module>'
      end

      # Parses the source file in `path`, returning the AST if successful.
      def parse_file(path)
        location = SourceLocation.new(1, 1, SourceFile.new(path))

        source = begin
          File.read(path)
        rescue => error
          diagnostics.error(error.message, location)
          return
        end

        parser = Parser.new(source, path)

        begin
          parser.parse
        rescue Parser::ParseError => error
          diagnostics.error(error.message, parser.location)
          nil
        end
      end

      def diagnostics
        @state.diagnostics
      end

      def dynamic_type
        Type::Dynamic.new
      end
    end
  end
end
