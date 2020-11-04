# frozen_string_literal: true

module Inkoc
  module AST
    class NewInstance
      include TypeOperations
      include Predicates
      include Inspect

      attr_reader :name, :attributes, :location

      def initialize(name, attributes, location)
        @name = name
        @attributes = attributes
        @location = location
      end

      def visitor_method
        :on_new_instance
      end

      def self_type?
        @name == Config::SELF_TYPE
      end
    end
  end
end
