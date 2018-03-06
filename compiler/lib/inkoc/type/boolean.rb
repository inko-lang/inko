# frozen_string_literal: true

module Inkoc
  module Type
    class Boolean
      include Inspect
      include Predicates
      include ObjectOperations
      include GenericTypeOperations
      include TypeCompatibility

      attr_reader :attributes, :implemented_traits
      attr_accessor :name, :prototype

      def initialize(name: Config::BOOLEAN_CONST, prototype: nil)
        @name = name
        @prototype = prototype
        @attributes = SymbolTable.new
        @implemented_traits = Set.new
      end

      def new_shallow_instance(*)
        self
      end

      def type_parameters
        TypeParameterTable.new
      end

      def boolean?
        true
      end

      def type_compatible?(other)
        valid = super

        if valid
          true
        else
          other.boolean?
        end
      end

      def lookup_method(name, *)
        super.or_else { lookup_method_from_traits(name) }
      end
    end
  end
end
