# frozen_string_literal: true

module Inkoc
  module Codegen
    class Module
      attr_reader :name, :body, :literals

      def initialize(name, body, literals = Literals.new)
        @name = name
        @body = body
        @literals = literals
      end
    end
  end
end
