%%machine aeon_lexer;

%% write data;

pub fn lex<F: FnMut(Token)>(input: &str, mut callback: F) -> Result<(), ()> {
    let data = input.as_bytes();

    let mut line   = 1;
    let mut column = 1;

    let mut ts  = 0;
    let mut te  = 0;
    let mut act = 0;
    let eof     = input.len();

    let mut p       = 0;
    let mut pe      = input.len();
    let mut cs: i32 = 0;

    %% write init;
    %% write exec;

    Ok(())
}

%%{
    action advance_line {
        line += 1;
        column = 1;
    }

    action advance_column {
        column += 1;
    }

    newline = ('\r\n' | '\n') @advance_line;

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

    main := |*
        comment | newline;

        docstring => {
            emit!(Docstring, data, ts + 2, te - 2, line, column, 4, callback);
        };

        integer => { emit!(Int, data, ts, te, line, column, 0, callback); };
        float   => { emit!(Float, data, ts, te, line, column, 0, callback); };

        dstring => {
            emit_string!(data, ts, te, line, column, "\\\"", "\"", callback);
        };

        sstring => {
            emit_string!(data, ts, te, line, column, "\\'", "'", callback);
        };

        ivar => {
            emit!(InstanceVariable, data, ts + 1, te, line, column, 1, callback);
        };

        identifier => {
            emit!(Identifier, data, ts, te, line, column, 0, callback);
        };

        constant => {
            emit!(Constant, data, ts, te, line, column, 0, callback);
        };

        any => advance_column;
    *|;
}%%

// vim: set ft=ragel:
