# frozen_string_literal: true

module Inkoc
  module Formatter
    class Pretty
      HEADER = "%s %s\n %s %s on line %s, column %s\n"

      BOLD = "\e[1m%s\e[0m"
      CYAN = "\e[36m%s\e[0m"
      YELLOW = "\e[33m%s\e[0m"
      RED = "\e[31m%s\e[0m"

      def format(diagnostics)
        output = []

        diagnostics.each do |diag|
          output << format_diagnostic(diag)
        end

        output.join("\n")
      end

      def format_diagnostic(diag)
        level = level_label(diag)
        padding = diag.line.to_s.length + 3

        output = Kernel.format(
          HEADER,
          level,
          ansi(:bold, diag.message),
          '-->'.rjust(padding - 1),
          diag.path,
          ansi(:cyan, diag.line),
          ansi(:cyan, diag.column)
        )

        source_line = diag.file.lines[diag.line - 1]

        if source_line && !source_line.empty?
          line_num = ansi(:cyan, diag.line.to_s)

          output += '|'.rjust(padding)
          output += "\n #{line_num} | #{source_line.chomp}\n"
          output += '|'.rjust(padding)
          output += ' ' * diag.column
          output += ansi(:bold, ansi(:yellow, '^'))
          output += "\n"
        end

        output
      end

      def level_label(diag)
        if diag.error?
          ansi(:bold, ansi(:red, 'ERROR:'))
        else
          ansi(:bold, ansi(:yellow, 'WARNING:'))
        end
      end

      def ansi(kind, string)
        return string unless STDIN.tty?

        case kind
        when :bold
          Kernel.format(BOLD, string)
        when :cyan
          Kernel.format(CYAN, string)
        when :yellow
          Kernel.format(YELLOW, string)
        when :red
          Kernel.format(RED, string)
        else
          string
        end
      end
    end
  end
end
