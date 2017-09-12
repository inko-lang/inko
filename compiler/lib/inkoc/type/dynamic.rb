# frozen_string_literal: true

module Inkoc
  module Type
    class Dynamic
      include Inspect
      include ObjectOperations

      attr_reader :attributes, :implemented_traits, :type_parameters,
                  :type_parameter_instances

      attr_accessor :name, :prototype

      def initialize(prototype = nil)
        @name = 'Dynamic'
        @prototype = prototype
        @attributes = SymbolTable.new
        @implemented_traits = Set.new
        @type_parameters = {}
        @type_parameter_instances = {}
      end

      def new_instance
        self.class.new(self)
      end

      def responds_to_message?(*)
        true
      end

      def lookup_attribute(name)
        super.or_else { Symbol.new(name, Type::Dynamic.new) }
      end

      # Dynamic types are compatible with everything else.
      def type_compatible?(*)
        true
      end

      def dynamic?
        true
      end

      def regular_object?
        true
      end

      def type_name
        'Dynamic'
      end
    end
  end
end
