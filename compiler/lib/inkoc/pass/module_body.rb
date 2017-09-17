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
        on_module_body(ast, @module.body)

        [ast]
      end

      def on_module_body(node, body)
        body.add_connected_basic_block('prelude')

        self_local = body.define_self_local(@module.type)
        location = @module.location
        mod_name = set_array_of_strings(@module.name.parts, body, location)

        mod_reg = send_object_message(
          get_toplevel(body, location),
          Config::DEFINE_MODULE_MESSAGE,
          [mod_name],
          body,
          location
        )

        body.set_local(self_local, mod_reg, location)

        process_node(node, body)
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

        if ins && ins.return?
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
        end
      end

      def on_method(node, body)
        receiver = get_self(body, node.location)

        if node.required?
          define_required_method(node, receiver, body) if receiver.type.trait?
        else
          define_method(node, receiver, body)
        end
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
        type = receiver.type.lookup_method(name).type
        block = body.add_code_object(name, type, location)

        define_block_arguments(block, node.arguments)

        on_body(node.body, block)

        body.set_block(block, type, location)
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
        return on_raw_instruction(node, body) if raw_instruction?(node)
      end

      def on_raw_instruction(node, body)
        name = node.name
        callback = :"on_raw_#{node}"

        if respond_to?(callback)
          public_send(callback, node, body)
        else
          diagnostics.unknown_raw_instruction_error(name)
        end
      end

      def on_raw_get_toplevel(node, body)
        get_toplevel(body, node.location)
      end

      def raw_instruction?(node)
        node.receiver &&
          node.receiver.constant? &&
          node.receiver.name == Config::RAW_INSTRUCTION_RECEIVER
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

        body.send_object_message(reg, receiver, name_reg, arguments, location)
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
