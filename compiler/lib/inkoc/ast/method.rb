# frozen_string_literal: true

module Inkoc
  module AST
    class Method
      include TypeOperations
      include Predicates
      include Inspect

      attr_reader :name, :arguments, :type_parameters, :returns, :throws,
                  :type_requirements, :body, :location

      # name - The name of the method.
      # args - The arguments of the method.
      # returns - The return type of the method.
      # throws - The type being thrown by this method.
      # required - If the method is a required method in a trait.
      # type_requirements - Any method type requirements that are defined.
      # body - The body of the method.
      # loc - The SourceLocation of this method.
      def initialize(
        name,
        args,
        targs,
        returns,
        throws,
        required,
        type_requirements,
        body,
        loc
      )
        @name = name
        @arguments = args
        @type_parameters = targs
        @returns = returns
        @throws = throws
        @type_requirements = type_requirements
        @body = body
        @location = loc
        @required = required
        @type = nil
      end

      def required?
        @required
      end

      def visitor_method
        :on_method
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
