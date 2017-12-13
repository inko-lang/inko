# frozen_string_literal: true

module Inkoc
  module AST
    class ModuleDocumentation
      include Inspect
      include Predicates

      attr_reader :body, :location

      def initialize(body, location)
        @body = body
        @location = location
      end

      def visitor_method
        :on_module_documentation
      end
    end
  end
end
