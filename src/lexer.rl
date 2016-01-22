%%machine aeon_lexer;

%% write data;

pub fn lex() {
    let mut p = 0;
    let mut pe = 0;
    let mut cs: i32 = 0;

    %% write init;
    %% write exec;
}

%%{
    main := |*
        any;
    *|;
}%%

// vim: set ft=ragel:
