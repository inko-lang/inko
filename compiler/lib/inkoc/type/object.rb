# frozen_string_literal: true

module Inkoc
  module Type
    class Object
      include Inspect
      include ObjectOperations
      include GenericTypeOperations
      include TypeCompatibility

      attr_reader :attributes, :implemented_traits, :type_parameters,
                  :type_parameter_instances

      attr_accessor :name, :prototype

      def initialize(name = nil, prototype = nil)
        @name = name
        @prototype = prototype
        @attributes = SymbolTable.new
        @implemented_traits = Set.new
        @type_parameters = {}
        @type_parameter_instances = {}
      end

      def new_instance
        self.class.new(name, self)
      end

      def regular_object?
        true
      end

      def type_parameter_instances_compatible?(other)
        return false unless other.regular_object?
        return true if other.type_parameter_instances.empty?

        type_parameter_instances.all? do |name, type|
          other_type = other.lookup_type_parameter_instance(name)

          other_type ? type.type_compatible?(other_type) : false
        end
      end

      def type_compatible?(other)
        super && type_parameter_instances_compatible?(other)
      end
    end
  end
end
