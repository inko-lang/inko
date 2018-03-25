# frozen_string_literal: true

module Inkoc
  module AST
    class Method
      include TypeOperations
      include Predicates
      include Inspect

      attr_reader :name, :arguments, :type_parameters, :returns, :throws,
                  :method_bounds, :body, :location

      # name - The name of the method.
      # args - The arguments of the method.
      # returns - The return type of the method.
      # throws - The type being thrown by this method.
      # required - If the method is a required method in a trait.
      # method_bounds - Additional type requirements for this method.
      # body - The body of the method.
      # loc - The SourceLocation of this method.
      def initialize(
        name,
        args,
        targs,
        returns,
        throws,
        required,
        method_bounds,
        body,
        loc
      )
        @name = name
        @arguments = args
        @type_parameters = targs
        @returns = returns
        @throws = throws
        @method_bounds = method_bounds
        @body = body
        @location = loc
        @required = required
        @type = nil
      end

      def required?
        @required
      end

      def visitor_method
        required? ? :on_required_method : :on_method
      end

      def method?
        true
      end

      def block_type
        type
      end
    end
  end
end
