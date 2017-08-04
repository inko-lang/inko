# frozen_string_literal: true

module Inkoc
  module TIR
    class Module
      include Inspect

      attr_reader :name, :location, :body, :globals

      def initialize(name, location)
        @name = name
        @location = location
        @body = CodeObject.new(name, location)
        @globals = SymbolTable.new
      end
    end
  end
end
