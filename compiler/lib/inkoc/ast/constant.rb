# frozen_string_literal: true

module Inkoc
  module AST
    class Constant
      include TypeOperations
      include Predicates
      include Inspect

      attr_reader :name, :location, :receiver

      # name - The name of the constant as a String.
      # location - The SourceLocation of the constant.
      # receiver - The object to search for the constant.
      def initialize(name, receiver, location)
        @name = name
        @receiver = receiver
        @location = location
      end

      def constant?
        true
      end

      def qualified_name
        segments = []
        source = self

        while source
          segments << source.name
          source = source.receiver
        end

        segments
          .reverse
          .join(Config::MODULE_SEPARATOR)
      end

      def self_type?
        name == Config::SELF_TYPE
      end

      def dynamic_type?
        name == Config::DYNAMIC_TYPE
      end

      def never_type?
        name == Config::NEVER_TYPE
      end

      def visitor_method
        :on_constant
      end

      def define_variable_visitor_method
        :on_define_constant
      end
    end
  end
end
