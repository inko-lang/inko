# frozen_string_literal: true

module Support
  module Parser
    def parse_source(code)
      Inkoc::Parser.new(code).parse
    end
  end
end
