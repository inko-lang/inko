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

macro_rules! emit {
    ($kind: ident, $data: ident, $start: expr, $end: expr, $line: expr, $col: expr, $offset: expr, $callback: ident) => ({
        let value  = to_string!($data, $start, $end);
        let length = value.chars().count() + $offset;
        let token  = token!($kind, value, $line, $col);

        $col += length;

        $callback(token);
    });
}

macro_rules! emit_string {
    ($data: expr, $start: expr, $stop: expr, $line: expr, $col: expr, $find: expr, $replace: expr, $callback: expr) => ({
        let slice  = to_str!($data[$start + 1 .. $stop - 1]);
        let length = slice.chars().count() + 2;
        let string = slice.replace($find, $replace);
        let token  = token!(String, string, $line, $col);

        $callback(token);

        $col += length;
    });
}

#[derive(Debug)]
pub enum TokenType {
    Int,
    Float,
    String,
    Identifier,
    Constant,
    InstanceVariable,
    Docstring
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
}
