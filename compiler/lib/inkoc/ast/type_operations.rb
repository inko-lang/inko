# frozen_string_literal: true

module Inkoc
  module AST
    module TypeOperations
      attr_accessor :type

      def block_type
        Type::Void.new
      end
    end
  end
end
