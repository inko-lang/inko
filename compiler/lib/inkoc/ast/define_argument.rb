# frozen_string_literal: true

module Inkoc
  module AST
    class DefineArgument
      include Inspect

      attr_reader :name, :type, :default, :rest, :location

      # name - The name of the argument.
      # type - The type of the argument, if any.
      # default - The default value of the argument, if any.
      # rest - If the argument is a rest argument.
      # location - The SourceLocation of the argument.
      def initialize(name, type, default, rest, location)
        @name = name
        @type = type
        @default = default
        @rest = rest
        @location = location
      end

      def rest?
        @rest
      end

      def visitor_method
        :on_define_argument
      end
    end
  end
end
