# frozen_string_literal: true

module Inkoc
  module Type
    class Dynamic
      include Inspect
      include Predicates
      include ObjectOperations
      include TypeCompatibility
      include GenericTypeOperations

      def name
        'Dynamic'
      end
      alias type_name name

      def prototype=(*); end

      def required_traits
        Set.new
      end

      def required_method_types(*)
        []
      end

      def prototype
        nil
      end

      def attributes
        SymbolTable.new
      end

      def implemented_traits
        Set.new
      end

      def type_parameters
        TypeParameterTable.new
      end

      def new_instance(*)
        self
      end

      def responds_to_message?(*)
        true
      end

      def lookup_attribute(name)
        NullSymbol.new(name)
      end

      def type_compatible?(other)
        if other.optional?
          other.type.dynamic?
        else
          other.dynamic?
        end
      end
      alias strict_type_compatible? type_compatible?

      def dynamic?
        true
      end

      def regular_object?
        true
      end

      def generic_type?
        false
      end

      def implementation_of?(*)
        false
      end

      def ==(other)
        other.is_a?(Dynamic)
      end
    end
  end
end
