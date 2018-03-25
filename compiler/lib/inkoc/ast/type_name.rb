# frozen_string_literal: true

module Inkoc
  module AST
    class TypeName
      include TypeOperations
      include Predicates
      include Inspect

      attr_reader :name, :location, :type_parameters
      attr_accessor :optional

      # name - The name of the type.
      # location - The SourceLocation of the type.
      def initialize(name, type_parameters, location)
        @name = name
        @location = location
        @type_parameters = type_parameters
        @optional = false
      end

      def type_name
        name.name
      end

      def optional?
        @optional
      end

      def qualified_name
        name.qualified_name
      end

      def visitor_method
        case name.name
        when Config::SELF_TYPE
          :on_self_type
        when Config::DYNAMIC_TYPE
          :on_dynamic_type
        when Config::VOID_TYPE
          :on_void_type
        else
          :on_type_name
        end
      end
    end
  end
end
