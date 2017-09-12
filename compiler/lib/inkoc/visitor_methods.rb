# frozen_string_literal: true

module Inkoc
  module VisitorMethods
    def process_node(node, *args)
      callback = node.visitor_method

      public_send(callback, node, *args) if respond_to?(callback)
    end

    def process_nodes(nodes, *args)
      nodes.map { |node| process_node(node, *args) }
    end
  end
end
