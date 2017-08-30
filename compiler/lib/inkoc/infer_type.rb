# frozen_string_literal: true

module Inkoc
  module InferType
    def type_of_node(node, self_type, mod)
      case node
      when AST::Integer
        typedb.integer_type
      when AST::Float
        typedb.float_type
      when AST::String
        typedb.string_type
      when AST::Array
        Type::Array.new(typedb.array_prototype)
      when AST::HashMap
        Type::HashMap.new(typedb.hash_map_prototype)
      when AST::Attribute
        self_type.lookup_attribute(node.name).type
      when AST::Identifier
        name = node.name

        self_type.lookup_type(name)
          .or_else { mod.lookup_type(name) }
          .type
          .return_type
      when AST::Send
        rec_node = node.receiver

        rec_type =
          if rec_node
            type_of_node(rec_node, self_type, mod)
          else
            self_type
          end

        rec_type
          .lookup_method(node.name)
          .type
          .return_type
      when AST::Constant
        type_for_constant(node, [self_type, mod])
      else
        Type::Dynamic.new
      end
    end
  end
end
