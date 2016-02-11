#![allow(unused_parens)]
#![allow(unused_assignments)]
#![allow(non_upper_case_globals)]
#![allow(dead_code)]

use std::str;

macro_rules! to_str {
    ($bytes: expr) => (
        match str::from_utf8(&$bytes) {
            Ok(slice) => slice,
            Err(_)    => return Err(())
        }
    );
}

macro_rules! to_string {
    ($data: ident, $start: expr, $end: expr) => (
        to_str!($data[$start .. $end]).to_string();
    );
}

macro_rules! token {
    ($kind: ident, $value: expr, $line: expr, $col: expr) => (
        Token::new(TokenType::$kind, $value, $line, $col)
    );
}

macro_rules! yield_token {
    ($kind: ident, $value: expr, $line: expr, $col: expr, $length: expr, $callback: expr) => ({
        let token = token!($kind, $value, $line, $col);

        $callback(token);

        $col += $length;
    });
}

macro_rules! emit {
    ($kind: ident, $data: ident, $start: expr, $end: expr, $line: expr, $col: expr, $offset: expr, $callback: ident) => ({
        let value  = to_string!($data, $start, $end);
        let length = value.chars().count() + $offset;

        yield_token!($kind, value, $line, $col, length, $callback);
    });
}

macro_rules! emit_string {
    ($data: expr, $start: expr, $stop: expr, $line: expr, $col: expr, $find: expr, $replace: expr, $callback: expr) => ({
        let slice  = to_str!($data[$start + 1 .. $stop - 1]);
        let length = slice.chars().count() + 2;
        let string = slice.replace($find, $replace);

        yield_token!(String, string, $line, $col, length, $callback);
    });
}

macro_rules! emit_indent {
    ($kind: ident, $line: expr, $col: expr, $callback: expr) => (
        yield_token!($kind, "".to_string(), $line, $col, 0, $callback);
    );
}

#[derive(Debug)]
pub enum TokenType {
    Append,
    Arrow,
    BrackClose,
    BrackOpen,
    Colon,
    ColonColon,
    Comma,
    Constant,
    CurlyClose,
    CurlyOpen,
    Docstring,
    Dot,
    Equal,
    Float,
    Greater,
    Identifier,
    InstanceVariable,
    Int,
    Lower,
    Operator,
    ParenClose,
    ParenOpen,
    Pipe,
    String,
    Trait,
    Class,
    Def,
    Enum,
    Use,
    Import,
    As,
    Let,
    Mutable,
    Return,
    Super,
    Break,
    Continue,
    Public,
    Dynamic,
    Indent,
    Unindent
}

#[derive(Debug)]
pub struct Token {
    pub kind: TokenType,
    pub value: String,
    pub line: usize,
    pub column: usize
}

impl Token {
    pub fn new(kind: TokenType, val: String, line: usize, col: usize) -> Token {
        Token { kind: kind, value: val, line: line, column: col }
    }
}

include!(concat!(env!("OUT_DIR"), "/lexer.rs"));

#[cfg(test)]
mod tests {
    use super::*;

    macro_rules! assert_token {
        ($token: expr, $kind: ident, $value: expr, $line: expr, $col: expr) => (
            match $token.kind {
                TokenType::$kind => {
                    assert_eq!($token.value, $value);
                    assert_eq!($token.line, $line);
                    assert_eq!($token.column, $col);
                },
                _ => panic!("invalid token {:?}", $token)
            };
        );
    }

    macro_rules! assert_ok {
        ($result: ident) => ( assert!($result.is_ok()); );
    }

    macro_rules! tokenize {
        ($input: expr) => ({
            let mut tokens: Vec<Token> = Vec::new();

            let res = lex($input, |token| {
                tokens.push(token);
            });

            assert_ok!(res);

            tokens
        });
    }

    #[test]
    fn test_integers() {
        let tokens = tokenize!("10 10_5");

        assert_token!(tokens[0], Int, "10", 1, 1);
        assert_token!(tokens[1], Int, "10_5", 1, 4);
    }

    #[test]
    fn test_floats() {
        let tokens = tokenize!("10.5 10_5.5");

        assert_token!(tokens[0], Float, "10.5", 1, 1);
        assert_token!(tokens[1], Float, "10_5.5", 1, 6);
    }

    #[test]
    fn test_single_quote_strings() {
        let tokens = tokenize!("'foo' 'foo\\'bar'");

        assert_token!(tokens[0], String, "foo", 1, 1);
        assert_token!(tokens[1], String, "foo'bar", 1, 7);
    }

    #[test]
    fn test_double_quote_strings() {
        let tokens = tokenize!("\"hello\" \"hello\\\"world\"");

        assert_token!(tokens[0], String, "hello", 1, 1);
        assert_token!(tokens[1], String, "hello\"world", 1, 9);
    }

    #[test]
    fn test_identifiers() {
        let tokens = tokenize!("foo foö 한국어 _foo foo123 foo_bar");

        assert_token!(tokens[0], Identifier, "foo", 1, 1);
        assert_token!(tokens[1], Identifier, "foö", 1, 5);
        assert_token!(tokens[2], Identifier, "한국어", 1, 9);
        assert_token!(tokens[3], Identifier, "_foo", 1, 13);
        assert_token!(tokens[4], Identifier, "foo123", 1, 18);
        assert_token!(tokens[5], Identifier, "foo_bar", 1, 25);
    }

    #[test]
    fn test_constants() {
        let tokens = tokenize!("Foo Foö F한국어 Foo123 Foo_bar");

        assert_token!(tokens[0], Constant, "Foo", 1, 1);
        assert_token!(tokens[1], Constant, "Foö", 1, 5);
        assert_token!(tokens[2], Constant, "F한국어", 1, 9);
        assert_token!(tokens[3], Constant, "Foo123", 1, 14);
        assert_token!(tokens[4], Constant, "Foo_bar", 1, 21);
    }

    #[test]
    fn test_ivars() {
        let tokens = tokenize!("@foo @foö @한국어 @_foo @foo123 @foo_bar");

        assert_token!(tokens[0], InstanceVariable, "foo", 1, 1);
        assert_token!(tokens[1], InstanceVariable, "foö", 1, 6);
        assert_token!(tokens[2], InstanceVariable, "한국어", 1, 11);
        assert_token!(tokens[3], InstanceVariable, "_foo", 1, 16);
        assert_token!(tokens[4], InstanceVariable, "foo123", 1, 22);
        assert_token!(tokens[5], InstanceVariable, "foo_bar", 1, 30);
    }

    #[test]
    fn test_single_line_comment() {
        let tokens = tokenize!("# comment\nhello");

        assert_token!(tokens[0], Identifier, "hello", 2, 1);
    }

    #[test]
    fn test_docstring() {
        let tokens = tokenize!("/**/ /* foo */\n/* bar */ /* / */ /* * */");

        assert_token!(tokens[0], Docstring, "", 1, 1);
        assert_token!(tokens[1], Docstring, " foo ", 1, 6);
        assert_token!(tokens[2], Docstring, " bar ", 2, 1);
        assert_token!(tokens[3], Docstring, " / ", 2, 11);
        assert_token!(tokens[4], Docstring, " * ", 2, 19);
    }

    #[test]
    fn test_sigils() {
        let tokens = tokenize!("| :: -> ( ) [ ] { } = , . += + * - % / < >");

        assert_token!(tokens[0], Pipe, "|", 1, 1);
        assert_token!(tokens[1], ColonColon, "::", 1, 3);
        assert_token!(tokens[2], Arrow, "->", 1, 6);
        assert_token!(tokens[3], ParenOpen, "(", 1, 9);
        assert_token!(tokens[4], ParenClose, ")", 1, 11);
        assert_token!(tokens[5], BrackOpen, "[", 1, 13);
        assert_token!(tokens[6], BrackClose, "]", 1, 15);
        assert_token!(tokens[7], CurlyOpen, "{", 1, 17);
        assert_token!(tokens[8], CurlyClose, "}", 1, 19);
        assert_token!(tokens[9], Equal, "=", 1, 21);
        assert_token!(tokens[10], Comma, ",", 1, 23);
        assert_token!(tokens[11], Dot, ".", 1, 25);
        assert_token!(tokens[12], Append, "+=", 1, 27);
        assert_token!(tokens[13], Operator, "+", 1, 30);
        assert_token!(tokens[14], Operator, "*", 1, 32);
        assert_token!(tokens[15], Operator, "-", 1, 34);
        assert_token!(tokens[16], Operator, "%", 1, 36);
        assert_token!(tokens[17], Operator, "/", 1, 38);
        assert_token!(tokens[18], Lower, "<", 1, 40);
        assert_token!(tokens[19], Greater, ">", 1, 42);
    }

    #[test]
    fn test_keywords() {
        let tokens = tokenize!("trait class def enum use import as let mut return super break continue pub dyn");

        assert_token!(tokens[0], Trait, "trait", 1, 1);
        assert_token!(tokens[1], Class, "class", 1, 7);
        assert_token!(tokens[2], Def, "def", 1, 13);
        assert_token!(tokens[3], Enum, "enum", 1, 17);
        assert_token!(tokens[4], Use, "use", 1, 22);
        assert_token!(tokens[5], Import, "import", 1, 26);
        assert_token!(tokens[6], As, "as", 1, 33);
        assert_token!(tokens[7], Let, "let", 1, 36);
        assert_token!(tokens[8], Mutable, "mut", 1, 40);
        assert_token!(tokens[9], Return, "return", 1, 44);
        assert_token!(tokens[10], Super, "super", 1, 51);
        assert_token!(tokens[11], Break, "break", 1, 57);
        assert_token!(tokens[12], Continue, "continue", 1, 63);
        assert_token!(tokens[13], Public, "pub", 1, 72);
        assert_token!(tokens[14], Dynamic, "dyn", 1, 76);
    }

    #[test]
    fn test_indent_without_colon() {
        let tokens = tokenize!("foo\n  bar");

        assert_token!(tokens[0], Identifier, "foo", 1, 1);
        assert_token!(tokens[1], Identifier, "bar", 2, 3);
    }

    #[test]
    fn test_indent_with_colon() {
        let tokens = tokenize!("foo:\n  bar\nbaz");

        assert_token!(tokens[0], Identifier, "foo", 1, 1);
        assert_token!(tokens[1], Indent, "", 2, 1);
        assert_token!(tokens[2], Identifier, "bar", 2, 3);
        assert_token!(tokens[3], Unindent, "", 3, 1);
        assert_token!(tokens[4], Identifier, "baz", 3, 1);
    }

    #[test]
    fn test_multiple_indents_with_colon_eof() {
        let tokens = tokenize!("a:\n  b:\n    c:\n      d");

        assert_token!(tokens[0], Identifier, "a", 1, 1);
        assert_token!(tokens[1], Indent, "", 2, 1);
        assert_token!(tokens[2], Identifier, "b", 2, 3);
        assert_token!(tokens[3], Indent, "", 3, 1);
        assert_token!(tokens[4], Identifier, "c", 3, 5);
        assert_token!(tokens[5], Indent, "", 4, 1);
        assert_token!(tokens[6], Identifier, "d", 4, 7);
        assert_token!(tokens[7], Unindent, "", 4, 8);
        assert_token!(tokens[8], Unindent, "", 4, 8);
        assert_token!(tokens[9], Unindent, "", 4, 8);
    }

    #[test]
    fn test_multiple_indents_with_colon_explicit_unindent() {
        let tokens = tokenize!("a:\n  b\nc");

        assert_token!(tokens[0], Identifier, "a", 1, 1);
        assert_token!(tokens[1], Indent, "", 2, 1);
        assert_token!(tokens[2], Identifier, "b", 2, 3);
        assert_token!(tokens[3], Unindent, "", 3, 1);
        assert_token!(tokens[4], Identifier, "c", 3, 1);
    }

    #[test]
    fn test_indent_with_colon_single_line() {
        let tokens = tokenize!("foo: bar");

        assert_token!(tokens[0], Identifier, "foo", 1, 1);
        assert_token!(tokens[1], Indent, "", 1, 4);
        assert_token!(tokens[2], Identifier, "bar", 1, 6);
        assert_token!(tokens[3], Unindent, "", 1, 9);
    }

    #[test]
    fn test_hash_literal() {
        let tokens = tokenize!("{a:b}");

        assert_token!(tokens[0], CurlyOpen, "{", 1, 1);
        assert_token!(tokens[1], Identifier, "a", 1, 2);
        assert_token!(tokens[2], Colon, ":", 1, 3);
        assert_token!(tokens[3], Identifier, "b", 1, 4);
        assert_token!(tokens[4], CurlyClose, "}", 1, 5);
    }

    #[test]
    fn test_multi_line_hash_literal() {
        let tokens = tokenize!("{a:\nb}");

        assert_token!(tokens[0], CurlyOpen, "{", 1, 1);
        assert_token!(tokens[1], Identifier, "a", 1, 2);
        assert_token!(tokens[2], Colon, ":", 1, 3);
        assert_token!(tokens[3], Identifier, "b", 2, 1);
        assert_token!(tokens[4], CurlyClose, "}", 2, 2);
    }
}
