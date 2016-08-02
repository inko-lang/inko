%%machine aeon_lexer; # %

module Aeon
  class Lexer
    %% write data;

    # % fix highlight

    def initialize(data)
      @data  = data
      @ts    = 0
      @te    = 0
      @top   = 0
      @cs    = self.class.aeon_lexer_start
      @act   = 0
      @eof   = @data.bytesize
      @p     = 0
      @pe    = @eof

      @emit_unindent_eol = false
      @emit_indent       = false
      @indent_stack      = []
      @curly_count       = 0

      @line   = 1
      @column = 1
    end

    def lex
      token = nil

      _aeon_lexer_eof_trans          = self.class.send(:_aeon_lexer_eof_trans)
      _aeon_lexer_from_state_actions = self.class.send(:_aeon_lexer_from_state_actions)
      _aeon_lexer_index_offsets      = self.class.send(:_aeon_lexer_index_offsets)
      _aeon_lexer_indicies           = self.class.send(:_aeon_lexer_indicies)
      _aeon_lexer_key_spans          = self.class.send(:_aeon_lexer_key_spans)
      _aeon_lexer_to_state_actions   = self.class.send(:_aeon_lexer_to_state_actions)
      _aeon_lexer_trans_actions      = self.class.send(:_aeon_lexer_trans_actions)
      _aeon_lexer_trans_keys         = self.class.send(:_aeon_lexer_trans_keys)
      _aeon_lexer_trans_targs        = self.class.send(:_aeon_lexer_trans_targs)

      %% write exec;

      # % fix highlight

      return token if token

      if @emit_unindent_eol
        @emit_unindent_eol = false

        return indent_token(:Unindent)
      end

      while @indent_stack.length > 0
        @indent_stack.pop

        return indent_token(:Unindent)
      end

      nil
    end

    private

    def to_string(start, stop)
      return @data.byteslice(start, stop - start)
    end

    def new_token(type, value, length)
      token = Token.new(type, value, @line, @column)

      @column += length

      token
    end

    def token(type)
      value  = to_string(@ts, @te)
      length = value.length

      new_token(type, value, length)
    end

    def offset_token(type, start, stop, offset)
      value  = to_string(start, stop)
      length = value.length + offset

      new_token(type, value, length)
    end

    def string_token(find, replace, type)
      slice  = to_string(@ts + 1, @te - 1)
      length = slice.length + 2
      string = slice.gsub(find, replace)

      new_token(type, string, length)
    end

    def indent_token(type)
      Token.new(type, '', @line, @column)
    end

    %%{
      getkey (@data.getbyte(@p) || 0);

      variable p   @p;
      variable pe  @pe;
      variable eof @eof;
      variable ts  @ts;
      variable te  @te;
      variable act @act;
      variable cs  @cs;

      action advance_line {
        if @emit_unindent_eol
          @emit_unindent_eol = false

          token = indent_token(:Unindent)
        end

        @line  += 1;
        @column = 1;

        fnext line_start;

        if token
          fbreak;
        end
      }

      action advance_column {
        @column += 1
      }

      whitespace = [ \t];
      newline    = ('\r\n' | '\n');

      unicode     = any - ascii;
      ident_chunk = ([a-zA-Z0-9_] | unicode);
      identifier  = ([a-z_] | unicode) ident_chunk* ('!' | '?')?;
      constant    = upper ident_chunk*;
      ivar        = '@' identifier;

      action emit_identifier {
        token = token(:Identifier)
        fbreak;
      }

      integer = ('+' | '-')? digit+ ('_' digit+)*;
      float   = integer '.' integer;

      squote  = "'";
      dquote  = '"';
      sstring = squote ( [^'\\] | /\\./ )* squote;
      dstring = dquote ( [^"\\] | /\\./ )* dquote;

      comment   = '#' ^newline+;
      docstring = '/*' any* :>> '*/';

      colon  = ':';
      dcolon = colon colon;
      lparen = '(';
      rparen = ')';
      lbrack = '[';
      rbrack = ']';
      lcurly = '{';
      rcurly = '}';
      assign = '=';
      eq     = '==';
      comma  = ',';
      dot    = '.';
      arrow  = '->';

      action emit_comma {
        token = token(:Comma)
        fbreak;
      }

      action emit_lparen {
        token = token(:ParenOpen)
        fbreak;
      }

      action emit_rparen {
        token = token(:ParenClose)
        fbreak;
      }

      plus_assign = '+=';
      min_assign  = '-=';
      div_assign  = '/=';
      mod_assign  = '%=';
      mul_assign  = '*=';
      pipe_assign = '|=';
      amp_assign  = '&=';
      bit_excl_or_assign = '^=';

      plus_prefix = '+@';
      min_prefix  = '-@';

      comp   = '<=>';
      not    = '!';
      neq    = '!=';
      lt     = '<';
      gt     = '>';
      lte    = '<=';
      gte    = '>=';
      pipe   = '|';
      plus   = '+';
      minus  = '-';
      div    = '/';
      modulo = '%';
      pow    = '**';
      star   = '*';
      and    = '&&';
      or     = '||';
      amp    = '&';

      shift_left  = '<<';
      shift_right = '>>';

      bit_excl_or = '^';
      range_inc   = '..';
      range_excl  = '...';

      # Machine used for processing the start of a line.
      line_start := |*
        # Start of a line with leading whitespace. The amount of spaces before
        # the first non-space character is used to calculate/compare the
        # indentation.
        whitespace+ any => {
          indent = (@te - @ts) - 1
          last   = @indent_stack.last || 0

          # We only want to emit an indent when explicitly told. This allows
          # for code such as:
          #
          #     foo
          #       .bar
          #       .baz
          #
          # Which will then be treated as:
          #
          #     foo.bar.baz
          if @emit_indent
            @emit_indent = false

            if indent > last
              token = indent_token(:Indent)

              @indent_stack.push(indent)
            end
          elsif indent < last
            token = indent_token(:Unindent)

            @indent_stack.pop
          end

          @column += indent

          fhold;
          fnext main;

          if token
            fbreak;
          end
        };

        # Non whitespace characters at the start of a new line.
        ^space => {
          last = @indent_stack.last || 0

          if last > 0
            token = indent_token(:Unindent)

            @indent_stack.pop

            fhold;
            fbreak;
          else
            fhold;
            fnext main;
          end
        };

        # Empty lines are ignored.
        space => {
          fhold;
          fnext main;
        };
      *|;

      compile_flag := |*
        space;

        identifier => emit_identifier;
        lparen     => emit_lparen;
        rparen     => emit_rparen;
        comma      => emit_comma;

        ']' => {
          token = token(:CompileFlagClose)
          fnext main;
          fbreak;
        };
      *|;

      main := |*
        comment;

        '![' => {
          token = token(:CompileFlagOpen)
          fnext compile_flag;
          fbreak;
        };

        'trait' => {
          token = token(:Trait)
          fbreak;
        };

        'class' => {
          token = token(:Class)
          fbreak;
        };

        'extends' => {
          token = token(:Extends)
          fbreak;
        };

        'mod' => {
          token = token(:Module)
          fbreak;
        };

        'def' => {
          token = token(:Def)
          fbreak;
        };

        'enum' => {
          token = token(:Enum)
          fbreak;
        };

        'member' => {
          token = token(:Member)
          fbreak;
        };

        'use' => {
          token = token(:Use)
          fbreak;
        };

        'import' => {
          token = token(:Import)
          fbreak;
        };

        'as' => {
          token = token(:As)
          fbreak;
        };

        'let' => {
          token = token(:Let)

          fbreak;
        };

        'mut' => {
          token = token(:Mutable)
          fbreak;
        };

        'return' => {
          token = token(:Return)
          fbreak;
        };

        'super' => {
          token = token(:Super)
          fbreak;
        };

        'break' => {
          token = token(:Break)
          fbreak;
        };

        'next' => {
          token = token(:Next)
          fbreak;
        };

        'dyn' => {
          token = token(:Dynamic)
          fbreak;
        };

        'type' => {
          token = token(:Type)
          fbreak;
        };

        'true' => {
          token = token(:True)
          fbreak;
        };

        'false' => {
          token = token(:False)
          fbreak;
        };

        'self' => {
          token = token(:Self)
          fbreak;
        };

        docstring  => {
          token = offset_token(:Docstring, @ts + 2, @te - 2, 4)
          fbreak;
        };

        integer => {
          token = token(:Integer)
          fbreak;
        };

        float => {
          token = token(:Float)
          fbreak;
        };

        dstring => {
          token = string_token("\\\"", "\"", :DoubleString)
          fbreak;
        };

        sstring => {
          token = string_token("\\'", "'", :SingleString)
          fbreak;
        };

        ivar => {
          token = offset_token(:InstanceVariable, @ts + 1, @te, 1)
          fbreak;
        };

        identifier => emit_identifier;

        constant => {
          token = token(:Constant)
          fbreak;
        };

        dcolon => {
          token = token(:ColonColon)
          fbreak;
        };

        arrow => {
          token = token(:Arrow)
          fbreak;
        };

        lparen => emit_lparen;
        rparen => emit_rparen;

        lbrack => {
          token = token(:BrackOpen)
          fbreak;
        };

        rbrack => {
          token = token(:BrackClose)
          fbreak;
        };

        plus_assign => {
          token = token(:PlusAssign)
          fbreak;
        };

        min_assign => {
          token = token(:MinAssign)
          fbreak;
        };

        div_assign => {
          token = token(:DivAssign)
          fbreak;
        };

        mod_assign => {
          token = token(:ModAssign)
          fbreak;
        };

        mul_assign => {
          token = token(:MulAssign)
          fbreak;
        };

        bit_excl_or_assign => {
          token = token(:BitwiseExclOrAssign)
          fbreak;
        };

        pipe_assign => {
          token = token(:PipeAssign)
          fbreak;
        };

        amp_assign => {
          token = token(:AmpersandAssign)
          fbreak;
        };

        neq => {
          token = token(:NotEqual)
          fbreak;
        };

        comp => {
          token = token(:Compare)
          fbreak;
        };

        not => {
          token = token(:Not)
          fbreak;
        };

        assign => {
          token = token(:Assign)
          fbreak;
        };

        eq => {
          token = token(:Equal)
          fbreak;
        };

        comma => emit_comma;

        dot => {
          token = token(:Dot)
          fbreak;
        };

        plus => {
          token = token(:Plus)
          fbreak;
        };

        minus => {
          token = token(:Minus)
          fbreak;
        };

        plus_prefix => {
          token = token(:PlusPrefix)
          fbreak;
        };

        min_prefix => {
          token = token(:MinusPrefix)
          fbreak;
        };

        div => {
          token = token(:Div)
          fbreak;
        };

        modulo => {
          token = token(:Modulo)
          fbreak;
        };

        star => {
          token = token(:Star)
          fbreak;
        };

        and => {
          token = token(:And)
          fbreak;
        };

        or => {
          token = token(:Or)
          fbreak;
        };

        pipe => {
          token = token(:Pipe)
          fbreak;
        };

        amp => {
          token = token(:Ampersand)
          fbreak;
        };

        range_inc => {
          token = token(:RangeInc)
          fbreak;
        };

        range_excl => {
          token = token(:RangeExcl)
          fbreak;
        };

        lte => {
          token = token(:LowerEqual)
          fbreak;
        };

        gte => {
          token = token(:GreaterEqual)
          fbreak;
        };

        pow => {
          token = token(:Power)
          fbreak;
        };

        bit_excl_or => {
          token = token(:BitwiseExclOr)
          fbreak;
        };

        shift_left => {
          token = token(:ShiftLeft)
          fbreak;
        };

        shift_right => {
          token = token(:ShiftRight)
          fbreak;
        };

        lt => {
          token = token(:Lower)
          fbreak;
        };

        gt => {
          token = token(:Greater)
          fbreak;
        };

        lcurly => {
          token = token(:CurlyOpen)

          @curly_count += 1

          fbreak;
        };

        rcurly => {
          token = token(:CurlyClose)

          @curly_count -= 1

          fbreak;
        };

        # foo: bar
        colon whitespace* ^newline => {
          if @curly_count == 0
            @emit_unindent_eol = true

            token = indent_token(:Indent)

            @column += (@te - @ts) - 1
          else
            token = offset_token(:Colon, @ts, @ts + 1, 0)

            # The above return token already increments the column by 1,
            # so we have to manually add one _less_.
            @column += (@te - @ts) - 2
          end

          fhold;
          fbreak;
        };

        # foo:
        # ...
        colon whitespace* newline => {
          if @curly_count > 0
            token = offset_token(:Colon, @ts, @ts + 1, 0)
          end

          @line  += 1;
          @column = 1;

          if token
            fbreak;
          elsif @curly_count == 0
            @emit_indent = true

            fnext line_start;
          end
        };

        newline => advance_line;
        any     => advance_column;
      *|;
    }%%
  end
end
