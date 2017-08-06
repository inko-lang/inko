# frozen_string_literal: true

module Inkoc
  module AST
    class Block
      include Inspect

      attr_reader :arguments, :body, :throws, :returns, :location

      # arguments - The arguments of the block.
      # body - The body of the block as a Body node.
      # returns - The return type of the block.
      # throws - The type that may be thrown.
      # location - The SourceLocation of the block.
      def initialize(arguments, returns, throws, body, location)
        @arguments = arguments
        @returns = returns
        @throws = throws
        @body = body
        @location = location
      end

      def tir_process_node_method
        :on_block
      end
    end
  end
end
