# frozen_string_literal: true

module Inkoc
  module AST
    class TraitImplementation
      include TypeOperations
      include Predicates
      include Inspect

      attr_reader :trait_name, :object_name, :body, :location

      attr_accessor :block_type

      # trait_name - The name of the trait being implemented.
      # object_name - The name of the object being implemented.
      # body - The body of the implementation.
      # location - The SourceLocation of the implementation.
      def initialize(trait_name, object_name, body, location)
        @trait_name = trait_name
        @object_name = object_name
        @body = body
        @location = location
      end

      def visitor_method
        :on_trait_implementation
      end
    end
  end
end
