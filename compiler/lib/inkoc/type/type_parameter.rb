# frozen_string_literal: true

module Inkoc
  module Type
    class TypeParameter
      include Inspect

      attr_reader :name, :required_traits, :required_methods

      # name - The name of the type parameter as a String.
      # required_traits - The traits that have to be implemented for this
      #                   parameter.
      def initialize(name, required_traits = Set.new)
        @name = name
        @required_methods = SymbolTable.new
        @required_traits = required_traits
      end

      def define_required_method(block_type)
        @required_methods.define(block_type.name, block_type)
      end

      def dynamic?
        false
      end

      def optional?
        false
      end

      def block?
        false
      end

      def regular_object?
        false
      end

      def trait?
        false
      end

      def type_parameter?
        true
      end

      def type_compatible?(other)
        other.is_a?(self.class) &&
          required_traits == other.required_traits &&
          required_methods == other.required_methods
      end

      def strict_type_compatible?(other)
        return false if other.dynamic?

        type_compatible?(other)
      end

      def type_name
        tname = name

        if required_traits.any?
          trait_names = required_traits.map(&:type_name).join(' + ')
          tname = "#{base}: #{trait_names}"
        end

        tname
      end
    end
  end
end
