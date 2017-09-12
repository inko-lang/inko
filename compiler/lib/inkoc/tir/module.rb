# frozen_string_literal: true

module Inkoc
  module TIR
    class Module
      include Inspect

      attr_reader :name, :location, :body, :globals, :type, :config

      def initialize(name, location)
        @name = name
        @location = location
        @body = CodeObject.new(name, location)
        @globals = SymbolTable.new
        @type = Type::Object.new(name.to_s)
        @config = ModuleConfig.new
      end

      def lookup_type(name)
        @type.lookup_type(name)
      end

      def lookup_attribute(name)
        @type.lookup_attribute(name)
      end

      def import_bootstrap?
        @config.import_bootstrap?
      end

      def import_prelude?
        @config.import_prelude?
      end
    end
  end
end
