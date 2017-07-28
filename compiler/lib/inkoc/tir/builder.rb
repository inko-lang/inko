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

        build(qname, path)
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

      # Builds the body of a module.
      def module_body(ast, mod)
        import_bootstrap_module(mod)
        import_prelude_module(mod)
        define_module_object(mod)

        on_body(ast, mod.body, mod)
      end

      def define_module_object(mod)
        type = Type::Object.new("<module #{mod.name}>")
        self_local = mod.body.define_self_local(type)
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
        process_nodes(node.expressions, body, mod)
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
        receiver = get_self(body, node.location)

        get_attribute(receiver, node.name, body, node.location)
      end

      def on_constant(node, body, mod)
        name = node.name
        location = node.location

        if node.receiver
          receiver = process_node(node.receiver, body, mod)

          get_attribute(
            receiver,
            name,
            body,
            location,
            :undefined_constant_error
          )
        else
          symbol = mod.globals[name]

          diagnostics.undefined_constant_error(name, location) if symbol.nil?

          get_global(symbol, body, location)
        end
      end

      def on_define_type_alias(_node, _body, _mod)
        raise NotImplementedError
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

        diagnostics.mutable_constant_error(location) if node.mutable?

        rec_reg = get_self(body, location)
        value_reg = set_attribute(rec_reg, name, value, body, location)

        # Constants defined at the top-level should also be available as module
        # globals.
        if mod.body == body
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

      # Gets an attribute from a register.
      #
      # receiver - The register to get the attribute from.
      # name - The name of the attribute as a String.
      # error - The method to use for generating an error message if the
      #         attribute is not defined.
      def get_attribute(
        receiver,
        name,
        body,
        location,
        error = :undefined_attribute_error
      )
        rec_type = receiver.type
        attribute = rec_type.lookup_attribute(name)

        diagnostics.public_send(error, name, location) if attribute.nil?

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
        reg_type = return_type_of_message_send(receiver.type, name, location)
        register = body.register(reg_type)
        name_reg = set_string(name, body, location)

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

      # receiver - The type of the receiver of a message send.
      # name - The name of the message as a String.
      def return_type_of_message_send(receiver, name, location)
        symbol = receiver.lookup_method(name)
        type = symbol.type

        if type.block?
          type.return_type
        elsif symbol.any?
          diangostics.not_a_method_error(name, location)
          type
        else
          diagnostics.undefined_method_error(name, location)
          type
        end
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
