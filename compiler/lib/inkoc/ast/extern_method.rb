# frozen_string_literal: true

module Inkoc
  module AST
    class ExternMethod
      include TypeOperations
      include Predicates
      include Inspect

      attr_reader :name, :arguments, :returns, :throws, :location

      def initialize(name, arguments, returns, throws, location)
        @name = name
        @arguments = arguments
        @returns = returns
        @throws = throws
        @location = location
      end

      def visitor_method
        :on_extern_method
      end

      def type_parameters
        []
      end

      def yields
        nil
      end

      def method_bounds
        []
      end
    end
  end
end
