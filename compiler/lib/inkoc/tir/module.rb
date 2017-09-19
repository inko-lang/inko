# frozen_string_literal: true

module Inkoc
  module TIR
    class Module
      include Inspect

      attr_reader :name, :location, :body, :globals, :type, :config,
                  :bytecode_directory, :bytecode_file

      def initialize(name, location)
        @name = name
        @type = Type::Object.new(name.to_s)
        @location = location
        @body = CodeObject.new(name, @type, location)
        @globals = SymbolTable.new
        @config = ModuleConfig.new

        hash = Digest::SHA1.hexdigest(file_path_as_string)

        @bytecode_directory = hash[0..1]
        @bytecode_file = hash[1..-1] + Config::BYTECODE_EXT
      end

      def lookup_type(name)
        type.lookup_type(name)
      end

      def lookup_attribute(name)
        type.lookup_attribute(name)
      end

      def import_bootstrap?
        config.import_bootstrap?
      end

      def import_prelude?
        config.import_prelude?
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
