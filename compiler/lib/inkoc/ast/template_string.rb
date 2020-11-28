# frozen_string_literal: true

module Inkoc
  module AST
    class TemplateString
      include TypeOperations
      include Predicates
      include Inspect

      attr_reader :members, :location

      def initialize(members, location)
        @members = members
        @location = location
      end

      def visitor_method
        :on_template_string
      end
    end
  end
end
