# frozen_string_literal: true

module Inkoc
  module AST
    class Import
      include TypeOperations
      include Predicates
      include Inspect

      attr_reader :steps, :symbols, :location

      # steps - The steps that make up the module path to import.
      # symbols - The symbols to import, if any.
      # location - The SourceLocation of the import statement.
      def initialize(steps, symbols, location)
        @steps = steps
        @symbols = symbols
        @location = location
      end

      def import?
        true
      end

      def visitor_method
        :on_import
      end

      def qualified_name
        steps = @steps.each_with_object([]) do |step, array|
          array << step.name if step.identifier?
        end

        TIR::QualifiedName.new(steps)
      end

      def expression?
        false
      end
    end
  end
end
