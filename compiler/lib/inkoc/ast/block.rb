# frozen_string_literal: true

module Inkoc
  module AST
    class Block
      include Inspect

      attr_reader :arguments, :body, :return_type, :location

      # arguments - The arguments of the block.
      # body - The body of the block as a Body node.
      # return_type - The return type of the block.
      # location - The SourceLocation of the block.
      def initialize(arguments, body, return_type, location)
        @arguments = arguments
        @body = body
        @return_type = return_type
        @location = location
      end

      def tir_process_node_method
        :on_block
      end
    end
  end
end
