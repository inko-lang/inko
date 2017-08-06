# frozen_string_literal: true

module Inkoc
  module TIR
    class Module
      include Inspect

      attr_reader :name, :location, :body, :globals, :type

      def initialize(name, location)
        @name = name
        @location = location
        @body = CodeObject.new(name, location)
        @globals = SymbolTable.new
        @type = Type::Object.new(name.to_s)
      end

      def lookup_type(*args)
        @type.lookup_type(*args)
      end
    end
  end
end
