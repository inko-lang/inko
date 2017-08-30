# frozen_string_literal: true

module Inkoc
  module Type
    class Trait
      include Inspect
      include ObjectOperations
      include GenericTypeOperations
      include TypeCompatibility

      attr_reader :name, :attributes, :required_methods, :type_parameters,
                  :prototype, :implemented_traits

      def initialize(name, prototype = nil)
        @name = name
        @prototype = prototype
        @attributes = SymbolTable.new
        @required_methods = {}
        @implemented_traits = {}
        @type_parameters = {}
      end

      def new_instance
        self.class.new(name, self)
      end

      def define_required_method(block_type)
        @required_methods[block_type.name] = block_type
      end

      def lookup_method(name)
        @required_methods[name]
      end

      def trait?
        true
      end

      def type_compatible?(other)
        return true if self == other

        prototype_chain_compatible?(other)
      end
    end
  end
end
