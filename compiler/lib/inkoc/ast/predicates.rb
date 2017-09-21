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
    end
  end
end
