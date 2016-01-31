%%machine aeon_lexer;

%% write data;

pub fn lex<F: FnMut(Token)>(input: &str, mut callback: F) -> Result<(), ()> {
    let data = input.as_bytes();

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

    main := |*
        integer => { emit!(Int, data, ts, te, callback); };
        float   => { emit!(Float, data, ts, te, callback); };

        dstring => {
            let string = to_string!(data, ts + 1, te - 1).replace("\\\"", "\"");
            let token  = Token::String(string);

            callback(token);
        };

        sstring => {
            let string = to_string!(data, ts + 1, te - 1).replace("\\'", "'");
            let token  = Token::String(string);

            callback(token);
        };

        identifier => { emit!(Identifier, data, ts, te, callback); };
        ivar       => { emit!(InstanceVariable, data, ts + 1, te, callback); };
        constant   => { emit!(Constant, data, ts, te, callback); };

        any;
    *|;
}%%

// vim: set ft=ragel:
