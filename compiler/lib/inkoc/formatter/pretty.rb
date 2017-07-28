# frozen_string_literal: true

module Inkoc
  module Formatter
    class Pretty
      TEMPLATE = "%s %s\nFile: %s line %s, column %s\n"

      def format(diagnostics)
        output = []

        diagnostics.each do |diag|
          output << format_diagnostic(diag)
        end

        output.join("\n")
      end

      def format_diagnostic(diag)
        level = level_label(diag)
        text = ANSI.bold(diag.message)

        Kernel.format(TEMPLATE, level, text, diag.path, diag.line, diag.column)
      end

      def level_label(diag)
        if diag.error?
          ANSI.bold(ANSI.red('ERROR:'))
        else
          ANSI.bold(ANSI.yellow('WARNING:'))
        end
      end
    end
  end
end
