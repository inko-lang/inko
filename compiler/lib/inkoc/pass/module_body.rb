# frozen_string_literal: true

module Inkoc
  module Pass
    class ModuleBody
      include TypeVerification

      def initialize(state)
        @state = state
      end

      def run(ast, mod)
        on_module_body(ast, mod.body, mod)

        [mod]
      end

      def process_node(node, body, mod)
        callback = node.visitor_method

        public_send(callback, node, body, mod)
      end

      def process_nodes(nodes, body, mod)
        nodes.map { |node| process_node(node, body, mod) }
      end

      def on_module_body(node, body, mod)
        body.add_connected_basic_block('prelude')

        self_local = body.define_self_local(mod.type)
        location = mod.location
        mod_name = set_array_of_strings(mod.name.parts, body, location)

        mod_reg = send_object_message(
          body.get_toplevel(typedb.top_level, location),
          Config::DEFINE_MODULE_MESSAGE,
          [mod_name],
          body,
          location
        )

        body.set_local(self_local, mod_reg, location)

        process_node(node, body, mod)
      end

      def on_body(node, body, mod)
        body.add_connected_basic_block

        registers = process_nodes(node.expressions, body, mod)

        add_explicit_return(body)
        check_for_unreachable_blocks(body)

        registers
      end

      def add_explicit_return(body)
        ins = body.current_block.instructions.last
        loc = ins ? ins.location : body.location

        if ins && !ins.is_a?(TIR::Instruction::Return)
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

      def on_integer(node, body, _)
        body.set_integer(node.value, typedb.integer_type, node.location)
      end

      def on_float(node, body, _)
        body.set_float(node.value, typedb.float_type, node.location)
      end

      def on_string(node, body, _)
        set_string(node.value, body, node.location)
      end

      def on_self(node, body, _)
        get_self(body, node.location)
      end

      def on_identifier(node, body, mod)
        name = node.name
        loc = node.location

        if body.locals.defined?(name)
          body.get_local(body.locals[name], loc)
        elsif body.self_type.responds_to_message?(name)
          send_to_self(name, body, loc)
        elsif mod.globals.defined?(name)
          body.get_global(mod.globals[name], loc)
        else
          diagnostics.undefined_method_error(body.self_type, name, loc)
        end
      end

      def send_to_self(name, body, location)
        receiver = get_self(body, location)
        reg_type = receiver.type.message_return_type(name)
        reg = body.register(reg_type)
        name_reg = set_string(name, body, location)

        body.send_object_message(reg, receiver, name_reg, [], location)
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
