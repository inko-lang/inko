# frozen_string_literal: true

module Inkoc
  module Pass
    class DefineTypes
      include TypeLookup
      include VisitorMethods

      DeferredMethod = Struct.new(:ast, :self_type, :locals)

      attr_reader :module

      def initialize(mod, state)
        @module = mod
        @state = state
        @method_bodies = []
      end

      def diagnostics
        @state.diagnostics
      end

      def typedb
        @state.typedb
      end

      def define_type(node, self_type, locals)
        node.type = process_node(node, self_type, locals)
      end

      def define_types(nodes, self_type, locals)
        nodes.each { |node| define_type(node, self_type, locals) }
      end

      def run(ast)
        locals = ast.locals

        on_module_body(ast, locals)

        # Method bodies are processed last since they may depend on types
        # defined after the method itself is defined.
        @method_bodies.each do |method|
          process_deferred_method(method)
        end

        [ast]
      end

      def on_module_body(ast, locals)
        @module.type =
          if @module.define_module?
            define_module_type
          else
            typedb.top_level
          end

        define_type(ast, @module.type, locals)
      end

      def define_module_type
        top = typedb.top_level
        modules = top.lookup_attribute(Config::MODULES_ATTRIBUTE).type
        proto = top.lookup_attribute(Config::MODULE_TYPE).type
        type = Type::Object.new(name: @module.name.to_s, prototype: proto)

        modules.define_attribute(type.name, type, true)

        type
      end

      def on_body(node, self_type, locals)
        locals.define(Config::SELF_LOCAL, self_type)

        return_types = return_types_for_body(node, self_type, locals)
        first_type = return_types[0][0]

        return_types.each do |(type, location)|
          next if type.type_compatible?(first_type)

          diagnostics.type_error(first_type, type, location)
        end

        first_type
      end

      def return_types_for_body(node, self_type, locals)
        types = []
        last_type = nil

        node.expressions.each do |expr|
          type = define_type(expr, self_type, locals)

          next unless type

          location = expr.location
          last_type = [type, location]

          types.push([type, location]) if expr.return?
        end

        last_type ||= [typedb.nil_type, node.location]

        types << last_type
      end

      def on_integer(*)
        typedb.integer_type
      end

      def on_float(*)
        typedb.float_type
      end

      def on_string(*)
        typedb.string_type
      end

      def on_attribute(node, self_type, *)
        name = node.name
        symbol = self_type.lookup_attribute(name)

        if symbol.nil?
          diagnostics.undefined_attribute_error(self_type, name, node.location)
        end

        symbol.type
      end

      def on_constant(node, self_type, *)
        name = node.name
        symbol = self_type.lookup_attribute(name)
          .or_else { @module.lookup_attribute(name) }

        diagnostics.undefined_constant_error(name, node.location) if symbol.nil?

        symbol.type
      end

      def on_identifier(node, self_type, locals)
        name = node.name
        symbol = locals[name].or_else { self_type.lookup_method(name) }

        if symbol.nil?
          diagnostics.undefined_method_error(self_type, name, node.location)
        end

        symbol.type.return_type
      end

      def on_global(node, *)
        name = node.name
        symbol = @module.globals[name]

        diagnostics.undefined_constant_error(name, node.location) if symbol.nil?

        symbol.type
      end

      def on_self(_, self_type, *)
        self_type
      end

      def on_send(node, self_type, locals)
        if node.raw_instruction?
          return on_raw_instruction(node, self_type, locals)
        end

        name = node.name
        rec_type =
          if node.receiver
            define_type(node.receiver, self_type, locals)
          else
            self_type
          end

        explicit_arg_types = node.arguments.map do |arg|
          define_type(arg, self_type, locals)
        end

        return rec_type if rec_type.dynamic?

        # TODO: handle message sends to type parameters

        symbol = rec_type.lookup_method(node.name)

        unless symbol.type.block?
          diagnostics.undefined_method_error(rec_type, name, node.location)

          return symbol.type
        end

        method_type = symbol.type
        expected_arg_types = method_type.argument_types_without_self

        explicit_arg_types.each_with_index do |arg_type, index|
          exp = expected_arg_types[index]
          loc = node.arguments[index].location

          if exp.generated_trait? && !arg_type.implements_trait?(exp)
            diagnostics
              .generated_trait_not_implemented_error(exp, arg_type, loc)
          elsif !arg_type.type_compatible?(exp)
            diagnostics.type_error(exp, arg_type, loc)
          end
        end

        method_type
          .initialized_return_type(rec_type, [self_type, *explicit_arg_types])
      end

      def on_raw_instruction(node, self_type, locals)
        callback = node.raw_instruction_visitor_method

        # Although we don't directly use the argument types here we still want
        # to store them in every node so we can access them later on.
        node.arguments.each { |arg| define_type(arg, self_type, locals) }

        if respond_to?(callback)
          public_send(callback, node, self_type, locals)
        else
          diagnostics.unknown_raw_instruction_error(node.name, node.location)
          typedb.nil_type
        end
      end

      def on_raw_get_toplevel(*)
        typedb.top_level
      end

      def on_raw_set_attribute(node, *)
        node.arguments[2].type
      end

      def on_raw_set_object(node, *)
        proto =
          if (proto_node = node.arguments[1])
            proto_node.type
          end

        Type::Object.new(prototype: proto)
      end

      def on_raw_integer_to_string(*)
        typedb.string_type
      end

      def on_raw_stdout_write(*)
        typedb.integer_type
      end

      def on_raw_get_true(*)
        typedb.boolean_type
      end

      alias on_raw_get_false on_raw_get_true

      def on_return(node, self_type, locals)
        if node.value
          define_type(node.value, self_type, locals)
        else
          typedb.nil_type
        end
      end

      def on_throw(node, self_type, locals)
        define_type(node.value, self_type, locals)
      end

      def on_try(node, self_type, locals)
        exp_type = define_type(node.expression, self_type, locals)
        else_type = if node.else_body
                      define_type(node.else_body, self_type, locals)
                    end

        if else_type && !else_type.type_compatible?(exp_type)
          diagnostics.type_error(exp_type, else_type, node.else_body.location)
        end

        exp_type
      end

      def on_object(node, self_type, *)
        name = node.name
        top = typedb.top_level

        proto =
          if (sym = top.lookup_attribute(Config::OBJECT_CONST)) && sym.any?
            sym.type
          end

        type = Type::Object.new(name: name, prototype: proto)

        type.define_attribute(
          Config::OBJECT_NAME_INSTANCE_ATTRIBUTE,
          typedb.string_type
        )

        define_type_parameters(node.type_parameters, type)
        store_type(type, self_type)
        define_type(node.body, type, node.body.locals)
        define_block_type_for_object(node, type)

        type
      end

      def define_block_type_for_object(node, type)
        node.block_type = Type::Block.new(
          Config::BLOCK_NAME,
          typedb.block_prototype,
          returns: node.body.type
        )

        node.block_type.define_self_argument(type)
      end

      def on_trait(node, self_type, *)
        name = node.name
        type = Type::Trait.new(name: name, prototype: trait_prototype)

        define_type_parameters(node.type_parameters, type)

        node.required_traits.each do |trait|
          trait_type = type_for_constant(trait, [self_type, @module])

          type.required_traits << trait_type if trait_type.trait?
        end

        store_type(type, self_type)
        define_type(node.body, type, node.body.locals)
        define_block_type_for_object(node, type)

        type
      end

      def on_trait_implementation(node, self_type, *)
        trait = type_for_constant(node.trait_name, [self_type, @module])
        object = type_for_constant(node.object_name, [self_type, @module])
        loc = node.location

        define_type(node.body, object, node.body.locals)
        define_block_type_for_object(node, object)

        traits_implemented = required_traits_implemented?(object, trait, loc)
        methods_implemented = required_methods_implemented?(object, trait, loc)

        if traits_implemented && methods_implemented
          object.implemented_traits << trait
        end

        object
      end

      def required_traits_implemented?(object, trait, location)
        trait.required_traits.each do |req_trait|
          next if object.trait_implemented?(req_trait)

          diagnostics
            .uninplemented_trait_error(trait, object, req_trait, location)

          return false
        end

        true
      end

      def required_methods_implemented?(object, trait, location)
        trait.required_methods.each do |method|
          next if object.method_implemented?(method)

          diagnostics.unimplemented_method_error(method.type, object, location)

          return false
        end
      end

      def on_method(node, self_type, *)
        type = Type::Block.new(node.name, typedb.block_prototype)

        block_signature(node, type, self_type, node.body.locals)

        if node.required?
          if self_type.trait?
            self_type.define_required_method(type)
          else
            diagnostics.define_required_method_on_non_trait_error(node.location)
          end
        else
          store_type(type, self_type)
        end

        @method_bodies << DeferredMethod.new(node, self_type, node.body.locals)

        type
      end

      def process_deferred_method(method)
        node = method.ast
        body = node.body

        define_type(body, method.self_type, method.locals)

        expected_type = node.type.return_type
        inferred_type = body.type
        expected_type = method.self_type if expected_type.self_type?

        return if inferred_type.type_compatible?(expected_type)

        diagnostics
          .return_type_error(expected_type, inferred_type, node.location)
      end

      def on_block(node, self_type, *)
        type = Type::Block.new(Config::BLOCK_NAME, typedb.block_prototype)

        block_signature(node, type, self_type, node.body.locals)
        define_type(node.body, self_type, node.body.locals)

        rtype = node.body.type

        type.returns = rtype if type.returns.dynamic?

        unless rtype.type_compatible?(type.return_type)
          diagnostics.return_type_error(type.returns, rtype, node.location)
        end

        type
      end

      def on_define_variable(node, self_type, locals)
        callback = node.variable.define_variable_visitor_method
        vtype = define_type(node.value, self_type, locals)

        public_send(callback, node, self_type, vtype, locals)

        node.variable.type = vtype
      end

      def on_define_constant(node, self_type, value_type, *)
        store_type(value_type, self_type, node.variable.name)
      end

      def on_define_attribute(node, self_type, value_type, *)
        self_type.define_attribute(node.variable.name, value_type)
      end

      def on_define_local(node, _, value_type, locals)
        locals.define(node.variable.name, value_type, node.mutable?)
      end

      def on_reassign_variable(node, self_type, locals)
        callback = node.variable.reassign_variable_visitor_method
        vtype = define_type(node.value, self_type, locals)

        public_send(callback, node, self_type, vtype, locals)

        node.variable.type = vtype
      end

      def on_reassign_attribute(node, self_type, value_type, *)
        existing_type = self_type.lookup_attribute(node.variable.name).type

        return if value_type.type_compatible?(existing_type)

        diagnostics.type_error(existing_type, value_type, node.value.location)
      end

      def on_reassign_local(node, _, value_type, locals)
        name = node.variable.name
        local = locals[name]
        existing_type = local.type

        return if value_type.type_compatible?(existing_type)

        diagnostics.type_error(existing_type, value_type, node.value.location)
      end

      def block_signature(node, type, self_type, locals)
        define_type_parameters(node.type_parameters, type)
        define_arguments(node.arguments, type, self_type, locals)
        define_return_type(node, type, self_type)
        define_throw_type(node, type, self_type)
      end

      def define_arguments(arguments, block_type, self_type, locals)
        block_type.define_self_argument(self_type)

        arguments.each do |arg|
          val_type = type_for_argument_value(arg, self_type, locals)
          def_type = defined_type_for_argument(arg, block_type, self_type)

          # If both an explicit type and default value are given we need to make
          # sure the two are compatible.
          if argument_types_incompatible?(def_type, val_type)
            diagnostics.type_error(def_type, val_type, arg.default.location)
          end

          arg_name = arg.name
          arg_type = def_type || val_type || Type::Dynamic.new

          if arg.default
            block_type.define_argument(arg_name, arg_type)
          elsif arg.rest?
            block_type.define_rest_argument(arg_name, arg_type)
          else
            block_type.define_required_argument(arg_name, arg_type)
          end

          arg.type = arg_type

          locals.define(arg_name, arg_type)
        end
      end

      def define_return_type(node, block_type, self_type)
        rnode = node.returns

        unless rnode
          block_type.returns = Type::Dynamic.new
          return
        end

        if rnode.self_type?
          block_type.returns = Type::SelfType.new
          return
        end

        block_type.returns = wrap_optional_type(
          rnode,
          type_for_constant(rnode, [block_type, self_type, @module])
        )
      end

      def define_throw_type(node, block_type, self_type)
        return unless node.throws

        block_type.throws = wrap_optional_type(
          node.returns,
          type_for_constant(node.throws, [block_type, self_type, @module])
        )
      end

      def type_for_argument_value(arg, self_type, locals)
        define_type(arg.default, self_type, locals) if arg.default
      end

      def defined_type_for_argument(arg, block_type, self_type)
        return unless arg.type

        wrap_optional_type(
          arg.type,
          type_for_constant(arg.type, [block_type, self_type, @module])
        )
      end

      def argument_types_incompatible?(defined_type, value_type)
        defined_type && value_type && !defined_type.type_compatible?(value_type)
      end

      def store_type(type, self_type, name = type.name)
        self_type.define_attribute(name, type)

        @module.globals.define(name, type) if module_scope?(self_type)
      end

      def module_scope?(self_type)
        self_type == @module.type
      end

      def wrap_optional_type(node, type)
        node.optional? ? Type::Optional.new(type) : type
      end

      def trait_prototype
        typedb.top_level.lookup_attribute(Config::TRAIT_CONST).type
      end

      def define_type_parameters(arguments, type)
        proto = trait_prototype

        arguments.each do |arg_node|
          required_traits = arg_node.required_traits.map do |node|
            type_for_constant(node, [type, self.module])
          end

          trait = Type::Trait
            .new(name: arg_node.name, prototype: proto, generated: true)

          trait.required_traits.merge(required_traits)
          type.define_type_parameter(trait.name, trait)
        end
      end
    end
  end
end
