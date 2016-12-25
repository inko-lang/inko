module Inko
  class Token
    attr_reader :type, :value, :line, :column

    def initialize(type, value, line, column)
      @type   = type
      @value  = value
      @line   = line
      @column = column
    end
  end
end
