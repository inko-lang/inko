# frozen_string_literal: true

module Inkoc
  module Type
    module Predicates
      def generic_type?
        false
      end

      def optional?
        false
      end

      def block?
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
    end
  end
end
