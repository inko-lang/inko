# frozen_string_literal: true

module Inkoc
  module AST
    class TypeName
      include TypeOperations
      include Predicates
      include Inspect

      attr_reader :name, :location, :type_parameters
      attr_accessor :optional, :late_binding

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

      def late_binding?
        @late_binding
      end

      def qualified_name
        name.qualified_name
      end

      def visitor_method
        case name.name
        when Config::SELF_TYPE
          late_binding? ? :on_self_type_with_late_binding : :on_self_type
        when Config::DYNAMIC_TYPE
          :on_dynamic_type
        when Config::NEVER_TYPE
          :on_never_type
        else
          :on_type_name
        end
      end
    end
  end
end
