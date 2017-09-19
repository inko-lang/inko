# frozen_string_literal: true

module Inkoc
  module Pass
    class CodeWriter
      def initialize(mod, state)
        @module = mod
        @state = state
        @bytecode_directory =
          @state.config.target.join(@module.bytecode_directory)
      end

      def run(compiled_code)
        create_directory

        File.open(bytecode_file, 'wb') do |handle|
          handle.write(serialize(compiled_code))
        end

        []
      end

      def create_directory
        @bytecode_directory.mkpath
      end

      def bytecode_file
        @bytecode_directory.join(@module.bytecode_file)
      end

      def serialize(compiled_code)
        Codegen::Serializer.new.generate(compiled_code)
      end
    end
  end
end
