# frozen_string_literal: true

module Inkoc
  module AST
    class ReopenObject
      include TypeOperations
      include Predicates
      include Inspect

      attr_reader :name, :body, :location
      attr_accessor :block_type

      # name - The name of the object being implemented.
      # body - The body of the implementation.
      # location - The SourceLocation of the implementation.
      def initialize(name, body, location)
        @name = name
        @body = body
        @location = location
        @block_type = nil
      end

      def visitor_method
        :on_reopen_object
      end
    end
  end
end
