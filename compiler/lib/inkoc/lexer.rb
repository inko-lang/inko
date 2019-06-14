# frozen_string_literal: true

module Inkoc
  # Lexer that breaks up Inko source code into a series of tokens.
  class Lexer
    attr_reader :line, :column, :file

    IDENTIFIERS = {
      'let' => :let,
      'mut' => :mut,
      'object' => :object,
      'trait' => :trait,
      'import' => :import,
      'return' => :return,
      'self' => :self,
      'def' => :define,
      'do' => :do,
      'throw' => :throw,
      'else' => :else,
      'try' => :try,
      'as' => :as,
      'impl' => :impl,
      'for' => :for,
      'lambda' => :lambda,
      'where' => :where
    }.freeze

    SPECIALS = Set.new(
      [
        '!', '@', '#', '$', '%', '^', '&', '*', '(', ')',
        '-', '+', '=', '\\', ':', ';', '"', '\'', '<', '>', '/',
        ',', '.', ' ', "\r", "\n", '|', '[', ']'
      ]
    ).freeze

    NUMBER_RANGE = '0'..'9'
    NUMBER_ALLOWED_LETTERS = %w[a b c d e f A B C D E F x _]

    # We allocate this once so we don't end up wasting allocations every time we
    # consume a peeked value.
    NULL_TOKEN = NullToken.new.freeze

    def initialize(input, file_path = Pathname.new('(eval)'))
      @input = input.chars
      @position = 0
      @line = 1
      @column = 1
      @peeked = NULL_TOKEN
      @file = SourceFile.new(file_path)
    end

    # Returns the next available token, if any.
    #
    # This method will consume any previously peeked tokens before consuming
    # more input.
    def advance
      if @peeked.nil?
        advance_raw
      else
        value = @peeked
        @peeked = NULL_TOKEN

        value
      end
    end

    # Returns the next available token without advancing.
    def peek
      @peeked = advance_raw if @peeked.nil?

      @peeked
    end

    # Skips the current token and returns the next one.
    def skip_and_advance
      advance
      advance
    end

    # Returns true if the next token is of the given type.
    def next_type_is?(type)
      peek.type == type
    end

    # rubocop: disable Metrics/AbcSize
    # rubocop: disable Metrics/CyclomaticComplexity
    # rubocop: disable Metrics/BlockLength
    # rubocop: disable Metrics/PerceivedComplexity
    def advance_raw
      loop do
        char = @input[@position]

        case char
        when '@' then return attribute
        when '#'
          if (token = comment)
            return token
          end
        when NUMBER_RANGE then return number
        when '{' then return curly_open
        when '}' then return curly_close
        when '(' then return paren_open
        when ')' then return paren_close
        when '\'' then return single_string
        when '"' then return double_string
        when ':' then return colons
        when '/' then return div
        when '%' then return modulo_or_hash_open
        when '^' then return bitwise_xor
        when '&' then return bitwise_and_or_boolean_and
        when '|' then return bitwise_or_or_boolean_or
        when '*' then return mul_or_pow
        when '-' then return sub_or_arrow_or_negative_number
        when '+' then return add
        when '=' then return assign_or_equal
        when '<' then return lower_or_shift_left
        when '>' then return greater_or_shift_right
        when '[' then return bracket_open
        when ']' then return bracket_close
        when '!' then return not_equal_or_type_args_open_or_throws
        when '.' then return dot_or_range
        when ',' then return comma
        when "\r" then carriage_return
        when "\n" then advance_line
        when ' ', "\t" then advance_one
        when '_' then return starts_with_underscore
        when '?' then return question_mark
        else
          return NULL_TOKEN if SPECIALS.include?(char)
          return identifier_or_keyword if char && char == char.downcase
          return constant if char && char == char.upcase

          return NULL_TOKEN
        end
      end
    end
    # rubocop: enable Metrics/AbcSize
    # rubocop: enable Metrics/CyclomaticComplexity
    # rubocop: enable Metrics/BlockLength
    # rubocop: enable Metrics/PerceivedComplexity

    def carriage_return
      advance_line

      # If we're followed by a \n we'll just consume it so we don't advance the
      # line twice.
      @position += 1 if @input[@position] == "\n"
    end

    def starts_with_underscore
      start = @position + 1

      loop do
        char = @input[start]

        return NULL_TOKEN unless char

        if char == '_'
          start += 1
        else
          return identifier_or_keyword if char == char.downcase
          return constant if char == char.upcase
        end
      end
    end

    def identifier_or_keyword
      start, stop = advance_until_special
      token = new_token_or_null_token(:identifier, start, stop)
      ident_mapping = IDENTIFIERS[token.value]

      token.type = ident_mapping if ident_mapping

      if transform_to_try_bang?(token)
        transform_to_try_bang(token)
      else
        token
      end
    end

    def transform_to_try_bang?(token)
      token.type == :try && @input[@position] == '!'
    end

    def transform_to_try_bang(token)
      @position += 1
      @column += 1

      token.value = 'try!'
      token.type = :try_bang

      token
    end

    def constant
      start, stop = advance_until_special

      new_token_or_null_token(:constant, start, stop)
    end

    def attribute
      start = @position

      @position += 1

      _, stop = advance_until_special

      new_token_or_null_token(:attribute, start, stop)
    end

    def comment
      case @input[@position + 1]
      when '#'
        doc_comment
      when '!'
        module_comment
      else
        consume_comment_line
      end
    end

    def doc_comment
      consume_doc_comment(:documentation)
    end

    def module_comment
      consume_doc_comment(:module_documentation)
    end

    def consume_doc_comment(type)
      @position += 2
      @column += 2

      ignore_spaces

      start = @position

      consume_comment_line(advance_newline: false)

      token = new_token_or_null_token(type, start, @position)

      advance_line

      token
    end

    def consume_comment_line(advance_newline: true)
      loop do
        char = @input[@position]

        return unless char

        if char == "\n"
          advance_line if advance_newline
          return
        end

        @position += 1
        @column += 1
      end
    end

    def ignore_spaces
      loop do
        case @input[@position]
        when ' ', "\t"
          @position += 1
          @column += 1
        else
          return
        end
      end
    end

    # rubocop: disable Metrics/CyclomaticComplexity
    def number(skip_first: false)
      start = @position
      type = :integer

      @position += 1 if skip_first

      next_char = @input[@position + 1]
      is_hex = @input[@position] == '0' && (next_char == 'x' || next_char == 'X')

      loop do
        case @input[@position]
        when '.'
          next_char = @input[@position + 1]

          break unless NUMBER_RANGE.cover?(next_char)

          type = :float

          @position += 1
        when 'e', 'E'
          if is_hex
            @position += 1
          else
            type = :float
            next_char = @input[@position + 1]
            @position += next_char == '+' ? 2 : 1
          end
        when NUMBER_RANGE, *NUMBER_ALLOWED_LETTERS
          @position += 1
        else
          break
        end
      end

      token = new_token(type, start, @position)
      token.value.delete!('_')

      token
    end
    # rubocop: enable Metrics/CyclomaticComplexity

    def curly_open
      new_token(:curly_open, @position, @position += 1)
    end

    def curly_close
      new_token(:curly_close, @position, @position += 1)
    end

    def paren_open
      new_token(:paren_open, @position, @position += 1)
    end

    def paren_close
      new_token(:paren_close, @position, @position += 1)
    end

    def single_string
      string_with_quote("'", "\\'")
    end

    def double_string
      string_with_quote('"', '\\"', true)
    end

    # rubocop: disable Metrics/CyclomaticComplexity
    # rubocop: disable Metrics/PerceivedComplexity
    def string_with_quote(quote, escaped, unescape_special = false)
      # Skip the opening quote
      @position += 1

      start = @position
      has_escape = false
      has_special = false
      in_escape = false
      replace_backslash = false

      loop do
        char = @input[@position]

        break unless char

        @position += 1

        if char == quote && in_escape
          has_escape = true

          next
        elsif char == '\\'
          has_special = true

          if in_escape
            in_escape = false
            replace_backslash = true
          else
            in_escape = true
          end

          next
        end

        in_escape = false if in_escape

        break if char == quote
      end

      token = new_token(:string, start, @position - 1)

      token.value.gsub!(escaped, quote) if has_escape

      if has_special && unescape_special
        token.value.gsub!(
          /\\t|\\r|\\n|\\e|\\0/,
          '\t' => "\t",
          '\n' => "\n",
          '\r' => "\r",
          '\e' => "\e",
          '\0' => "\0"
        )
      end

      token.value.gsub!('\\\\', '\\') if replace_backslash

      @column += 2

      token
    end
    # rubocop: enable Metrics/PerceivedComplexity
    # rubocop: enable Metrics/CyclomaticComplexity

    def colons
      start = @position

      type, incr =
        @input[@position + 1] == ':' ? [:colon_colon, 2] : [:colon, 1]

      @position += incr

      new_token(type, start, @position)
    end

    def div
      operator(1, :div, :div_assign)
    end

    def operator(increment, type, assign_type = nil)
      start = @position
      token_type = type

      if @input[@position += increment] == '=' && assign_type
        @position += 1
        token_type = assign_type
      end

      new_token(token_type, start, @position)
    end

    def modulo_or_hash_open
      start = @position
      token_type = :mod

      case @input[@position += 1]
      when '['
        token_type = :hash_open
        @position += 1
      when '='
        token_type = :mod_assign
        @position += 1
      end

      new_token(token_type, start, @position)
    end

    def bitwise_xor
      operator(1, :bitwise_xor, :bitwise_xor_assign)
    end

    def bitwise_and_or_boolean_and
      if @input[@position + 1] == '&'
        operator(2, :and)
      else
        operator(1, :bitwise_and, :bitwise_and_assign)
      end
    end

    def bitwise_or_or_boolean_or
      if @input[@position + 1] == '|'
        operator(2, :or)
      else
        operator(1, :bitwise_or, :bitwise_or_assign)
      end
    end

    def mul_or_pow
      if @input[@position + 1] == '*'
        operator(2, :pow, :pow_assign)
      else
        operator(1, :mul, :mul_assign)
      end
    end

    def sub_or_arrow_or_negative_number
      peek = @input[@position + 1]

      if peek == '>'
        new_token(:arrow, @position, @position += 2)
      elsif NUMBER_RANGE.cover?(peek)
        number(skip_first: true)
      else
        operator(1, :sub, :sub_assign)
      end
    end

    def add
      operator(1, :add, :add_assign)
    end

    def assign_or_equal
      operator(1, :assign, :equal)
    end

    def not_equal_or_type_args_open_or_throws
      token_type = case @input[@position + 1]
                   when '='
                     :not_equal
                   when '('
                     :type_args_open
                   when '!'
                     :throws
                   else
                     return NULL_TOKEN
                   end

      new_token(token_type, @position, @position += 2)
    end

    def dot_or_range
      next_is_dot = @input[@position + 1] == '.'

      if next_is_dot && @input[@position + 2] == '.'
        new_token(:exclusive_range, @position, @position += 3)
      elsif next_is_dot
        new_token(:inclusive_range, @position, @position += 2)
      else
        new_token(:dot, @position, @position += 1)
      end
    end

    def comma
      new_token(:comma, @position, @position += 1)
    end

    def lower_or_shift_left
      if @input[@position + 1] == '<'
        operator(2, :shift_left, :shift_left_assign)
      else
        operator(1, :lower, :lower_equal)
      end
    end

    def greater_or_shift_right
      if @input[@position + 1] == '>'
        operator(2, :shift_right, :shift_right_assign)
      else
        operator(1, :greater, :greater_equal)
      end
    end

    def bracket_open
      new_token(:bracket_open, @position, @position += 1)
    end

    def bracket_close
      new_token(:bracket_close, @position, @position += 1)
    end

    def question_mark
      new_token(:question, @position, @position += 1)
    end

    def advance_line
      @position += 1
      @line += 1
      @column = 1
    end

    def advance_one
      @position += 1
      @column += 1
    end

    def advance_until_special
      start = @position

      loop do
        char = @input[@position]

        if char
          break if SPECIALS.include?(char)

          @position += 1
        else
          (@position - start).zero? ? return : break
        end
      end

      [start, @position]
    end

    def new_token_or_null_token(type, start, stop)
      start && stop ? new_token(type, start, stop) : NULL_TOKEN
    end

    def new_token(type, start, stop)
      location = current_location
      token = Token.new(type, @input[start...stop].join(''), location)

      @column += token.value.length

      token
    end

    def current_location
      SourceLocation.new(@line, @column, @file)
    end
  end
end
