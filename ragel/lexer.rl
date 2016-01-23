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
    integer = digit+ ('_' digit+)*;
    float   = integer '.' integer;

    main := |*
        integer => { emit!(Int, data, ts, te, callback); };
        float   => { emit!(Float, data, ts, te, callback); };

        any;
    *|;
}%%

// vim: set ft=ragel:
