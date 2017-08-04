# frozen_string_literal: true

module Inkoc
  module Formatter
    class Pretty
      HEADER = "%s %s\n %s %s on line %s, column %s\n"

      def format(diagnostics)
        output = []

        diagnostics.each do |diag|
          output << format_diagnostic(diag)
        end

        output.join("\n")
      end

      def format_diagnostic(diag)
        level = level_label(diag)
        padding = diag.column.to_s.length + 3

        output = Kernel.format(
          HEADER,
          level,
          ANSI.bold(diag.message),
          '-->'.rjust(padding - 1),
          diag.path,
          ANSI.cyan(diag.line),
          ANSI.cyan(diag.column)
        )

        source_line = diag.file.lines[diag.line - 1]

        if source_line && !source_line.empty?
          column = ANSI.cyan(diag.column.to_s)

          # TODO: clean up this garbage
          output += '|'.rjust(padding)
          output += "\n #{column} | #{source_line.chomp}\n"
          output += '|'.rjust(padding)
          output += ' ' * diag.column
          output += ANSI.bold(ANSI.yellow('^'))
          output += "\n"
        end

        output
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
