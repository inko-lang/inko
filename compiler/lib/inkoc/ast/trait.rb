# frozen_string_literal: true

module Inkoc
  module AST
    class Trait
      include TypeOperations
      include Predicates
      include Inspect

      attr_reader :name, :type_parameters, :body, :location,
                  :required_traits

      attr_accessor :block_type, :redefines

      # name - The name of the trait.
      # targs - The type arguments of the trait.
      # required - The other traits that must be implemented by the object
      #            implementing this trait.
      # body - The body of the trait.
      # location - The SourceLocation of the trait.
      def initialize(name, targs, required, body, location)
        @name = name
        @type_parameters = targs
        @required_traits = required
        @body = body
        @location = location
        @block_type = nil
        @redefines = false
      end

      def visitor_method
        :on_trait
      end
    end
  end
end
