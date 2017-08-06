# frozen_string_literal: true

module Inkoc
  module AST
    class Trait
      include Inspect

      attr_reader :name, :type_arguments, :body, :location

      # name - The name of the trait.
      # targs - The type arguments of the trait.
      # body - The body of the trait.
      # location - The SourceLocation of the trait.
      def initialize(name, targs, body, location)
        @name = name
        @type_arguments = targs
        @body = body
        @location = location
      end

      def tir_process_node_method
        :on_trait
      end
    end
  end
end
