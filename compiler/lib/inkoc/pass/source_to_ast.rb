# frozen_string_literal: true

module Inkoc
  module Pass
    class SourceToAst
      def initialize(mod, state)
        @module = mod
        @state = state
      end

      def run(source)
        parser = Parser.new(source, @module.file_path_as_string)

        begin
          [parser.parse]
        rescue Parser::ParseError => error
          @state.diagnostics.error(error.message, parser.location)
          nil
        end
      end
    end
  end
end
