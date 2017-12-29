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

        exprs = [
          # var obj = _INKOC.set_object(False, self)
          AST::DefineVariable.new(
            AST::Identifier.new('obj', loc),
            AST::Send.new(
              'set_object',
              AST::Constant.new(Config::RAW_INSTRUCTION_RECEIVER, nil, loc),
              [
                AST::Constant.new(Config::FALSE_CONST, nil, loc),
                AST::Self.new(loc)
              ],
              loc
            ),
            nil,
            true,
            loc
          ),

          # obj.init(...)
          AST::Send.new(
            Config::INIT_MESSAGE,
            AST::Identifier.new('obj', loc),
            arg_names,
            loc
          ),

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
