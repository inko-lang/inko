# frozen_string_literal: true

module Inkoc
  module AST
    class UnionType
      include Inspect

      attr_reader :members, :location

      # members - The members of the union type.
      # location - The SourceLocation of the union type.
      def initialize(members, location)
        @members = members
        @location = location
      end

      def tir_process_node_method
        :on_union_type
      end
    end
  end
end
