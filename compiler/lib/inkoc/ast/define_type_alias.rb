# frozen_string_literal: true

module Inkoc
  module AST
    class DefineTypeAlias
      include Inspect

      attr_reader :name, :type, :location

      # name - The type alias being defined.
      # type - The original type.
      # location - The SourceLocation of the type definition.
      def initialize(name, type, location)
        @name = name
        @type = type
        @location = location
      end

      def tir_process_node_method
        :on_define_type_alias
      end
    end
  end
end
