# frozen_string_literal: true

module Inkoc
  module TIR
    class Module
      include Inspect

      attr_reader :name, :location, :body, :globals, :config,
                  :bytecode_directory, :bytecode_file, :imports

      attr_accessor :type

      def initialize(name, location)
        @name = name
        @type = nil
        @location = location
        @body = CodeObject.new(name, Type::Block.new(name: name.to_s), location)
        @globals = SymbolTable.new
        @config = ModuleConfig.new

        hash = Digest::SHA1.hexdigest(file_path_as_string)

        @bytecode_directory = hash[0..1]
        @bytecode_file = hash[1..-1] + Config::BYTECODE_EXT
        @imports = []
      end

      def attributes
        type.attributes
      end

      def lookup_type(name)
        type.lookup_type(name) || type_of_global(name)
      end

      def lookup_attribute(name)
        type.lookup_attribute(name)
      end

      def type_of_attribute(name)
        type.type_of_attribute(name)
      end

      def responds_to_message?(name)
        lookup_attribute(name).any?
      end

      def type_of_global(name)
        return unless (global = globals[name]) && global.any?

        global.type
      end

      def global_defined?(name)
        globals.defined?(name)
      end

      def import_bootstrap?
        config.import_bootstrap?
      end

      def import_prelude?
        config.import_prelude?
      end

      def import_globals?
        config.import_globals?
      end

      def define_module?
        config.define_module?
      end

      def source_code
        location.file.path.read
      end

      def file_path_as_string
        location.file.path.to_s
      end

      def bytecode_import_path
        File.join(bytecode_directory, bytecode_file)
      end
    end
  end
end
