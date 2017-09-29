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
    end
  end
end
