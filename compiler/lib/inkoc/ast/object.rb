# frozen_string_literal: true

module Inkoc
  module AST
    class Object
      include TypeOperations
      include Predicates
      include Inspect

      attr_reader :name, :type_parameters, :body, :location,
                  :trait_implementations

      attr_accessor :block_type

      # name - The name of the object.
      # targs - The type arguments of the object.
      # implementations - The names of the traits to immediately implement.
      # body - The body of the object.
      # location - The SourceLocation of the object.
      def initialize(name, targs, implementations, body, location)
        @name = name
        @type_parameters = targs
        @trait_implementations = implementations
        @body = body
        @location = location
        @block_type = nil
      end

      def visitor_method
        :on_object
      end

      def object?
        true
      end
    end
  end
end
