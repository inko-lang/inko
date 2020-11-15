# frozen_string_literal: true

module Inkoc
  module TIR
    class Module
      include Inspect

      attr_reader :name, :location, :body, :globals, :config, :imports, :type

      def initialize(name, location)
        @name = name
        @type = nil
        @location = location
        @body = CodeObject.new(
          name,
          TypeSystem::Block.new(name: name.to_s, infer_throw_type: false),
          location
        )

        @globals = SymbolTable.new
        @imports = []
      end

      def type=(value)
        @type = value
        @body.type.self_type = value
      end

      def attributes
        type.attributes
      end

      def lookup_type(name)
        type.lookup_type(name) || lookup_global(name)
      end

      def lookup_attribute(name)
        type.lookup_attribute(name)
      end

      def lookup_global(name)
        symbol = globals[name]

        symbol.type if symbol.any?
      end

      def responds_to_message?(name)
        lookup_attribute(name).any?
      end

      def global_defined?(name)
        globals.defined?(name)
      end

      def import_bootstrap?
        name.to_s != Config.std_module_name(Config::BOOTSTRAP_MODULE)
      end

      def import_prelude?
        name = self.name.to_s

        name != Config.std_module_name(Config::BOOTSTRAP_MODULE) &&
          name != Config.std_module_name(Config::PRELUDE_MODULE)
      end

      def source_code
        location.file.path.open('r') do |file|
          file.each_char.to_a
        end
      end

      def file_path_as_string
        location.file.path.to_s
      end

      def lookup_any_type
        lookup_global(Config::ANY_TRAIT_CONST)
      end
    end
  end
end
