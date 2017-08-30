# frozen_string_literal: true

module Inkoc
  module Pass
    class PathToSource
      def initialize(state)
        @state = state
      end

      def run(path)
        [File.read(path), path]
      rescue => error
        location = SourceLocation.new(1, 1, SourceFile.new(path))
        @state.diagnostics.error(error.message, location)
        nil
      end
    end
  end
end
