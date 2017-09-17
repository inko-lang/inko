# frozen_string_literal: true

module Inkoc
  class TypeInference
    include TypeVerification
    include VisitorMethods

    def initialize(mod, state)
      @module = mod
      @state = state
    end

    alias infer process_node

    def on_integer(*)
      typedb.integer_type
    end

    def on_float(*)
      typedb.float_type
    end

    def on_string(*)
      typedb.string_type
    end

    def on_attribute(node, self_type)
      name = node.name
      symbol = self_type.lookup_attribute(name)

      diagnostics.undefined_attribute_error(name, node.location) if symbol.nil?

      symbol.type
    end

    def on_constant(node, self_type)
      name = node.name
      symbol = self_type.lookup_attribute(name)
        .or_else { @module.lookup_attribute(name) }

      diagnostics.undefined_attribute_error(name, node.location) if symbol.nil?

      symbol.type
    end

    def on_identifier(node, self_type)
      name = node.name

      symbol =
        if self_type.block?
          self_type.lookup_argument(name)
            .or_else { self_type.lookup_method(name) }
        else
          self_type.lookup_method(name)
        end

      if symbol.nil?
        diagnostics.undefined_method_error(self_type, name, node.location)
      end

      symbol.type.return_type
    end

    def on_send(node, self_type)
      name = node.name
      rec_type =
        node.receiver ? infer(node.receiver, self_type) : self_type

      symbol = rec_type.lookup_method(node.name)

      unless symbol.type.block?
        diagnostics.undefined_method_error(rec_type, name, node.location)

        return symbol.type
      end

      arg_types = node.arguments.map { |arg| infer(arg, self_type) }

      symbol.type.initialized_return_type(arg_types)
    end

    def on_global(node, *)
      name = node.name
      symbol = @module.globals[name]

      diagnostics.undefined_constant_error(name, node.location) if symbol.nil?

      symbol.type
    end

    def on_self(_, self_type)
      self_type
    end

    def on_define_variable(node, self_type)
      infer(node.value, self_type)
    end

    def on_return(node, self_type)
      node.value ? infer(node.value, self_type) : typedb.nil_type
    end

    def on_throw(node, self_type)
      infer(node.value, self_type)
    end

    def on_try(node, self_type)
      expression = node.expression.last_expression

      infer(expression, self_type)
    end

    def typedb
      @state.typedb
    end

    def diagnostics
      @state.diagnostics
    end
  end
end
