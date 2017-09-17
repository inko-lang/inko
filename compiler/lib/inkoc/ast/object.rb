# frozen_string_literal: true

module Inkoc
  module AST
    class Object
      include Predicates
      include Inspect

      attr_reader :name, :type_parameters, :trait_implementations, :body,
                  :location

      # name - The name of the object.
      # targs - The type arguments of the object.
      # impl - The trait implementations to add to this object.
      # body - The body of the object.
      # location - The SourceLocation of the object.
      def initialize(name, targs, impl, body, location)
        @name = name
        @type_parameters = targs
        @trait_implementations = impl
        @body = body
        @location = location
      end

      def visitor_method
        :on_object
      end
    end
  end
end
