# frozen_string_literal: true

module Inkoc
  module TIR
    class Module
      include Inspect

      attr_reader :name, :location, :body, :globals, :type, :config

      def initialize(name, location)
        @name = name
        @type = Type::Object.new(name.to_s)
        @location = location
        @body = CodeObject.new(name, @type, location)
        @globals = SymbolTable.new
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

      def source_code
        @location.file.path.read
      end

      def file_path_as_string
        @location.file.path.to_s
      end
    end
  end
end
