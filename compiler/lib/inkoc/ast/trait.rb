# frozen_string_literal: true

module Inkoc
  module AST
    class Trait
      include Predicates
      include Inspect

      attr_reader :name, :type_parameters, :body, :location,
                  :trait_implementations

      # name - The name of the trait.
      # targs - The type arguments of the trait.
      # impl - The other traits that must be implemented by the object
      #        implementing this trait.
      # body - The body of the trait.
      # location - The SourceLocation of the trait.
      def initialize(name, targs, impl, body, location)
        @name = name
        @type_parameters = targs
        @trait_implementations = impl
        @body = body
        @location = location
      end

      def visitor_method
        :on_trait
      end

      def hoist?
        true
      end

      def hoist_children?
        true
      end
    end
  end
end
