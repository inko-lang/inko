# frozen_string_literal: true

module Inkoc
  module AST
    class GlobImport
      include TypeOperations
      include Predicates
      include Inspect

      attr_reader :location

      def initialize(location)
        @location = location
      end

      def location_for_name
        location
      end

      def visitor_method
        :on_import_glob
      end

      def expose?
        true
      end
    end
  end
end
