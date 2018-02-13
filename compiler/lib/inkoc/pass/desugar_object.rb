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
        method = nil

        node.body.expressions.each do |expr|
          next unless expr.method?
          next unless expr.name == Config::INIT_MESSAGE

          method = create_new_from_init(expr)
        end

        node.body.expressions.push(method) if method

        process_node(node.body)
      end

      def on_trait(node)
        process_node(node.body)
      end

      # Generates a "new" method based on the signature of a corresponding
      # "init" method.
      def create_new_from_init(init)
        loc = init.location

        arg_names = init.arguments.map do |arg|
          AST::Identifier.new(arg.name, loc)
        end

        send_init = AST::Send.new(
          Config::INIT_MESSAGE,
          AST::Identifier.new('obj', loc),
          arg_names,
          loc
        )

        if init.throws
          send_init = AST::Try.new(send_init, AST::Body.new([], loc), nil, loc)
        end

        exprs = [
          # let mut obj = self.allocate
          AST::DefineVariable.new(
            AST::Identifier.new('obj', loc),
            AST::Send.new('allocate', AST::Self.new(loc), [], loc),
            nil,
            true,
            loc
          ),

          # obj.init(...)
          send_init,

          # return obj
          AST::Return.new(
            AST::Identifier.new('obj', loc),
            loc
          )
        ]

        body = AST::Body.new(exprs, loc)

        AST::Method.new(
          Config::NEW_MESSAGE,
          init.arguments,
          init.type_parameters,
          AST::Constant.new(Config::SELF_TYPE, nil, loc),
          init.throws,
          false,
          body,
          loc
        )
      end
    end
  end
end
