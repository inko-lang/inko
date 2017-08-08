# frozen_string_literal: true

module Inkoc
  module Type
    class TypeArgument
      include Inspect

      attr_reader :name, :required_traits

      # name - The name of the type argument as a String.
      # required_traits - The traits that have to be implemented for this type.
      def initialize(name, required_traits = [])
        @name = name
        @required_traits = required_traits
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
    end
  end
end
