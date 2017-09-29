# frozen_string_literal: true

module Inkoc
  module AST
    module Predicates
      def identifier?
        false
      end

      def constant?
        false
      end

      def import?
        false
      end

      def hoist?
        false
      end

      def method?
        false
      end

      def hoist_children?
        false
      end

      def variable_definition?
        false
      end

      def expression?
        true
      end

      def return?
        false
      end
    end
  end
end
