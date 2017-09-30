# frozen_string_literal: true

module Inkoc
  module AST
    class Object
      include TypeOperations
      include Predicates
      include Inspect

      attr_reader :name, :type_parameters, :body, :location

      attr_accessor :block_type

      # name - The name of the object.
      # targs - The type arguments of the object.
      # body - The body of the object.
      # location - The SourceLocation of the object.
      def initialize(name, targs, body, location)
        @name = name
        @type_parameters = targs
        @body = body
        @location = location
        @block_type = nil
      end

      def visitor_method
        :on_object
      end
    end
  end
end
