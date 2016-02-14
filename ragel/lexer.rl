%%machine aeon_lexer;

%% write data;

impl<'l> Lexer<'l> {
    pub fn lex(&mut self) -> Option<Token> {
        let ref data = self.data;

        let mut token: Option<Token> = None;

        %% write exec;

        if token.is_some() {
            return token;
        }

        if self.emit_unindent_eol {
            self.emit_unindent_eol = false;

            return indent_token!(Unindent, self);
        }

        while self.indent_stack.len() > 0 {
            self.indent_stack.pop();

            return indent_token!(Unindent, self);
        }

        None
    }
}

%%{
    variable p   self.p;
    variable pe  self.pe;
    variable eof self.eof;
    variable ts  self.ts;
    variable te  self.te;
    variable act self.act;
    variable cs  self.cs;

    action advance_line {
        if self.emit_unindent_eol {
            self.emit_unindent_eol = false;

            token = indent_token!(Unindent, self);
        }

        self.line  += 1;
        self.column = 1;

        fnext line_start;

        if token.is_some() {
            fnbreak;
        }
    }

    action advance_column {
        self.column += 1;
    }

    whitespace = [ \t];
    newline    = ('\r\n' | '\n');

    unicode    = any - ascii;
    identifier = ([a-z_] | unicode) ([a-zA-Z0-9_] | unicode)*;
    constant   = upper identifier?;
    ivar       = '@' identifier;

    integer = digit+ ('_' digit+)*;
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
    eq     = '=';
    comma  = ',';
    dot    = '.';
    arrow  = '->';
    append = '+=';
    lt     = '<';
    gt     = '>';
    pipe   = '|';

    operator = '+' | '-' | '/' | '%' | '*';

    # Machine used for processing the start of a line.
    line_start := |*
        # Start of a line with leading whitespace. The amount of spaces before
        # the first non-space character is used to calculate/compare the
        # indentation.
        whitespace+ any => {
            let indent = (self.te - self.ts) - 1;
            let last   = self.indent_stack.last().cloned().unwrap_or(0);

            // We only want to emit an indent when explicitly told. This allows
            // for code such as:
            //
            //     foo
            //       .bar
            //       .baz
            //
            // Which will then be treated as:
            //
            //     foo.bar.baz
            if self.emit_indent {
                self.emit_indent = false;

                if indent > last {
                    token = indent_token!(Indent, self);

                    self.indent_stack.push(indent);
                }
            }
            else if indent < last {
                token = indent_token!(Unindent, self);

                self.indent_stack.pop();
            }

            self.column += indent;

            fhold;
            fnext main;

            if token.is_some() {
                fnbreak;
            }
        };

        # Start of a new line without any leading characters.
        any => {
            let last = self.indent_stack.last().cloned().unwrap_or(0);

            if self.column < last {
                token = indent_token!(Unindent, self);

                self.indent_stack.pop();
            }

            fhold;
            fnext main;

            if token.is_some() {
                fnbreak;
            }
        };
    *|;

    main := |*
        comment;

        'trait' => {
            token = token!(Trait, self);
            fnbreak;
        };

        'class' => {
            token = token!(Class, self);
            fnbreak;
        };

        'def' => {
            token = token!(Def, self);
            fnbreak;
        };

        'enum' => {
            token = token!(Enum, self);
            fnbreak;
        };

        'use' => {
            token = token!(Use, self);
            fnbreak;
        };

        'import' => {
            token = token!(Import, self);
            fnbreak;
        };

        'as' => {
            token = token!(As, self);
            fnbreak;
        };

        'let' => {
            token = token!(Let, self);
            fnbreak;
        };

        'mut' => {
            token = token!(Mutable, self);
            fnbreak;
        };

        'return' => {
            token = token!(Return, self);
            fnbreak;
        };

        'super' => {
            token = token!(Super, self);
            fnbreak;
        };

        'break' => {
            token = token!(Break, self);
            fnbreak;
        };

        'continue' => {
            token = token!(Continue, self);
            fnbreak;
        };

        'pub' => {
            token = token!(Public, self);
            fnbreak;
        };

        'dyn' => {
            token = token!(Dynamic, self);
            fnbreak;
        };

        docstring  => {
            token = offset_token!(Docstring, self, self.ts + 2, self.te - 2, 4);
            fnbreak;
        };

        integer => {
            token = token!(Int, self);
            fnbreak;
        };

        float => {
            token = token!(Float, self);
            fnbreak;
        };

        dstring => {
            token = string_token!(self, "\\\"", "\"");
            fnbreak;
        };

        sstring => {
            token = string_token!(self, "\\'", "'");
            fnbreak;
        };

        ivar => {
            token = offset_token!(InstanceVariable, self, self.ts + 1, self.te, 1);
            fnbreak;
        };

        identifier => {
            token = token!(Identifier, self);
            fnbreak;
        };

        constant => {
            token = token!(Constant, self);
            fnbreak;
        };

        pipe => {
            token = token!(Pipe, self);
            fnbreak;
        };

        dcolon => {
            token = token!(ColonColon, self);
            fnbreak;
        };

        arrow => {
            token = token!(Arrow, self);
            fnbreak;
        };

        lparen => {
            token = token!(ParenOpen, self);
            fnbreak;
        };

        rparen => {
            token = token!(ParenClose, self);
            fnbreak;
        };

        lbrack => {
            token = token!(BrackOpen, self);
            fnbreak;
        };

        rbrack => {
            token = token!(BrackClose, self);
            fnbreak;
        };

        eq => {
            token = token!(Equal, self);
            fnbreak;
        };

        comma => {
            token = token!(Comma, self);
            fnbreak;
        };

        dot => {
            token = token!(Dot, self);
            fnbreak;
        };

        append => {
            token = token!(Append, self);
            fnbreak;
        };

        operator => {
            token = token!(Operator, self);
            fnbreak;
        };

        lt => {
            token = token!(Lower, self);
            fnbreak;
        };

        gt => {
            token = token!(Greater, self);
            fnbreak;
        };

        lcurly => {
            token = token!(CurlyOpen, self);

            self.curly_count += 1;

            fnbreak;
        };

        rcurly => {
            token = token!(CurlyClose, self);

            self.curly_count -= 1;

            fnbreak;
        };

        # foo: bar
        colon whitespace* ^newline => {
            if self.curly_count == 0 {
                self.emit_unindent_eol = true;

                token = indent_token!(Indent, self);

                self.column += (self.te - self.ts) - 1;
            }
            else {
                token = offset_token!(Colon, self, self.ts, self.ts + 1, 0);

                // The above return token already increments the column by 1,
                // so we have to manually add one _less_.
                self.column += (self.te - self.ts) - 2;
            }

            fhold;
            fnbreak;
        };

        # foo:
        # ...
        colon whitespace* newline => {
            if self.curly_count > 0 {
                token = offset_token!(Colon, self, self.ts, self.ts + 1, 0);
            }

            self.line  += 1;
            self.column = 1;

            if token.is_some() {
                fnbreak;
            }
            else if self.curly_count == 0 {
                self.emit_indent = true;

                fnext line_start;
            }
        };

        newline => advance_line;
        any     => advance_column;
    *|;
}%%

// vim: set ft=ragel:
