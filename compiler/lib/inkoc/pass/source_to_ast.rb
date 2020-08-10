# frozen_string_literal: true

module Inkoc
  module Pass
    class SourceToAst
      def initialize(compiler, mod)
        @module = mod
        @state = compiler.state
      end

      def run(source)
        parser = Parser.new(source, @module.file_path_as_string)

        ast =
          begin
            parser.parse
          rescue Parser::ParseError => error
            @state.diagnostics.error(error.message, parser.location)
            return
          end

        [ast]
      end
    end
  end
end
