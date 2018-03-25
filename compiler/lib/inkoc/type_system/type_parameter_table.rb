# frozen_string_literal: true

module Inkoc
  module TypeSystem
    # A table containing the type parameters defined for an object.
    class TypeParameterTable
      include Enumerable

      def initialize
        @parameters = []
        @mapping = {}
      end

      # Defines a new type parameter with the required traits.
      #
      # name - The name of the type parameter.
      # required_traits - The traits required by the type parameter.
      def define(name, required_traits = [])
        param = TypeParameter.new(name: name, required_traits: required_traits)

        @parameters << param
        @mapping[name] = param
      end

      # Returns the type parameter for the given name.
      #
      # name - The name of the type parameter.
      def [](name)
        @mapping[name]
      end

      # Returns the type parameter at the given position.
      #
      # index - The position to use.
      def at_index(index)
        @parameters[index]
      end

      # Returns true if this table defined the given type parameter.
      def defines?(parameter)
        self[parameter.name] == parameter
      end

      def each(&block)
        @parameters.each(&block)
      end

      def empty?
        @parameters.empty?
      end

      def any?
        @parameters.any?
      end

      def length
        @parameters.length
      end
    end
  end
end
