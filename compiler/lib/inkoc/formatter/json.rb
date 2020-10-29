# frozen_string_literal: true

module Inkoc
  module Formatter
    class Json
      def format(diagnostics)
        entries = diagnostics.map do |diag|
          format_diagnostic(diag)
        end

        JSON.generate(entries)
      end

      def format_diagnostic(diagnostic)
        {
          level: diagnostic.level,
          message: diagnostic.message,
          file: diagnostic.location.file.path.to_s,
          line: diagnostic.location.line,
          column: diagnostic.location.column
        }
      end
    end
  end
end
