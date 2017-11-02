# frozen_string_literal: true

module Inkoc
  module AST
    class BlockType
      include TypeOperations
      include Predicates
      include Inspect

      attr_reader :arguments, :returns, :throws, :location
      attr_accessor :optional

      def initialize(args, returns, throws, location)
        @arguments = args
        @returns = returns
        @throws = throws
        @location = location
        @optional = false
      end

      def optional?
        @optional
      end

      def visitor_method
        :on_block_type
      end

      def block_type?
        true
      end
    end
  end
end
