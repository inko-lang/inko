# frozen_string_literal: true

module Inkoc
  module Pass
    class DesugarObject
      include VisitorMethods

      def initialize(mod, state)
        @module = mod
        @state = state
      end

      def run(ast)
        process_node(ast)

        [ast]
      end

      def on_body(node)
        process_nodes(node.expressions)
      end

      def on_object(node)
        defines_new = node.body.expressions.any? do |expr|
          expr.method? && expr.name == Config::NEW_MESSAGE
        end

        # If an object defines a custom "new" method we don't want to overwrite
        # it.
        return if defines_new

        init = node.body.expressions.find do |expr|
          expr.method? && expr.name == Config::INIT_MESSAGE
        end

        method =
          if init
            create_new_from_init(init, init.location)
          else
            default_new(node.location)
          end

        method.static = true

        node.body.expressions.push(method)
        process_node(node.body)
      end

      def on_trait(node)
        process_node(node.body)
      end

      # Generates a default "new" method.
      def default_new(loc)
        exprs = [
          allocate_and_assign_object(loc),
          return_object(loc)
        ]

        body = AST::Body.new(exprs, loc)

        new_return_type = AST::TypeName
          .new(AST::Constant.new(Config::SELF_TYPE, nil, loc), [], loc)

        AST::Method.new(
          Config::NEW_MESSAGE,
          [],
          [],
          new_return_type,
          nil,
          false,
          [],
          body,
          loc
        )
      end

      # Generates a "new" method based on the signature of a corresponding
      # "init" method.
      def create_new_from_init(init, loc)
        arg_names = init.arguments.map do |arg|
          AST::Identifier.new(arg.name, loc)
        end

        # obj.init(...)
        send_init = AST::Send.new(
          Config::INIT_MESSAGE,
          AST::Identifier.new('obj', loc),
          [],
          arg_names,
          loc
        )

        if init.throws
          send_init = AST::Try.new(send_init, AST::Body.new([], loc), nil, loc)
        end

        exprs = [
          allocate_and_assign_object(loc),
          send_init,
          return_object(loc)
        ]

        body = AST::Body.new(exprs, loc)

        new_return_type = AST::TypeName
          .new(AST::Constant.new(Config::SELF_TYPE, nil, loc), [], loc)

        AST::Method.new(
          Config::NEW_MESSAGE,
          init.arguments,
          init.type_parameters,
          new_return_type,
          init.throws,
          false,
          init.method_bounds,
          body,
          loc
        )
      end

      def allocate_and_assign_object(loc)
        # var obj = _INKOC.set_object(FalseObject, self)
        AST::DefineVariable.new(
          AST::Identifier.new('obj', loc),
          AST::Send.new(
            'set_object',
            AST::Constant.new(Config::RAW_INSTRUCTION_RECEIVER, nil, loc),
            [],
            [
              AST::Constant.new(Config::FALSE_CONST, nil, loc),
              AST::Self.new(loc)
            ],
            loc
          ),
          nil,
          true,
          loc
        )
      end

      def return_object(loc)
        # return obj
        AST::Return.new(AST::Identifier.new('obj', loc), loc)
      end
    end
  end
end
