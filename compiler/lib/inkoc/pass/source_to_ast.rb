# frozen_string_literal: true

module Inkoc
  module Pass
    class SourceToAst
      def initialize(state)
        @state = state
      end

      def run(source, path)
        parser = Parser.new(source, path)

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
