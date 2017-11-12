# frozen_string_literal: true

module Inkoc
  module Type
    class Void
      include Inspect
      include Predicates

      def define_attribute(*); end

      def lookup_attribute(*)
        nil
      end

      def lookup_type(*)
        nil
      end

      def lookup_method(name)
        NullSymbol.new(name)
      end

      def implements_trait?(*)
        false
      end

      def implements_method?(*)
        false
      end

      def type_compatible?(*)
        true
      end

      def strict_type_compatible?(other)
        type_compatible?(other)
      end

      def message_return_type(*)
        self
      end

      def responds_to_message?(*)
        false
      end

      def attribute?(*)
        false
      end

      def void?
        true
      end

      def return_type
        self
      end

      def resolve_type(*)
        self
      end

      def physical_type?
        false
      end

      def if_physical_or_else
        yield
      end

      def type_name
        'Void'
      end
    end
  end
end
