# frozen_string_literal: true

module Inkoc
  module Type
    class Nil
      include Inspect
      include Predicates
      include ObjectOperations
      include GenericTypeOperations
      include TypeCompatibility

      attr_reader :implemented_traits, :attributes
      attr_accessor :prototype

      def initialize(prototype: nil)
        @prototype = prototype
        @implemented_traits = Set.new
        @attributes = SymbolTable.new
      end

      def regular_object?
        true
      end

      def name
        Config::NIL_CONST
      end
      alias type_name name

      def type_parameters
        TypeParameterTable.new
      end

      def new_instance(*)
        self
      end

      def type_compatible?(other)
        other.optional? ? true : super
      end
    end
  end
end
