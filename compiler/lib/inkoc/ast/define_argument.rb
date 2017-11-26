# frozen_string_literal: true

module Inkoc
  module AST
    class DefineArgument
      include TypeOperations
      include Predicates
      include Inspect

      attr_reader :name, :type, :rest, :location
      attr_accessor :default

      # name - The name of the argument.
      # type - The type of the argument, if any.
      # default - The default value of the argument, if any.
      # rest - If the argument is a rest argument.
      # mutable - If the argument is mutable or not.
      # location - The SourceLocation of the argument.
      def initialize(name, type, default, rest, mutable, location)
        @name = name
        @type = type
        @default = default
        @rest = rest
        @mutable = mutable
        @location = location
      end

      def rest?
        @rest
      end

      def mutable?
        @mutable
      end

      def visitor_method
        :on_define_argument
      end
    end
  end
end
