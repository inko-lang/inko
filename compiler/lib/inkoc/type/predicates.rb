# frozen_string_literal: true

module Inkoc
  module Type
    module Predicates
      def generic_type?
        false
      end

      def initialize_generic_type?
        false
      end

      def optional?
        false
      end

      def method?
        false
      end

      def block?
        false
      end

      def lambda?
        false
      end

      def closure?
        false
      end

      def regular_object?
        false
      end

      def generic_trait?
        false
      end

      def physical_type?
        true
      end

      def trait?
        false
      end

      def dynamic?
        false
      end

      def type_parameter?
        false
      end

      def self_type?
        false
      end

      def void?
        false
      end

      def constraint?
        false
      end

      def unresolved_constraint?
        false
      end

      def boolean?
        false
      end

      def nil_type?
        false
      end

      def downcast_to?(*)
        false
      end

      def resolve_type?
        false
      end
    end
  end
end
