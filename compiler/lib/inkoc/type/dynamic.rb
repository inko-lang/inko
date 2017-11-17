# frozen_string_literal: true

module Inkoc
  module Type
    class Dynamic
      include Inspect
      include Predicates
      include ObjectOperations
      include TypeCompatibility

      def name
        'Dynamic'
      end
      alias type_name name

      def prototype=(*); end

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
        {}
      end

      def type_parameter_instances
        {}
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
        other.dynamic?
      end
      alias strict_type_compatible? type_compatible?

      def dynamic?
        true
      end

      def regular_object?
        true
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
