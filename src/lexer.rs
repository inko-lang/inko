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

macro_rules! emit {
    ($token: ident, $data: ident, $start: expr, $end: expr, $callback: ident) => ({
        let value = to_string!($data, $start, $end);
        let token = Token::$token(value);

        $callback(token);
    });
}

#[derive(Debug)]
pub enum Token {
    Int(String),
    Float(String),
    String(String),
    Identifier(String),
    Constant(String),
    InstanceVariable(String),
    Docstring(String)
}

include!(concat!(env!("OUT_DIR"), "/lexer.rs"));

#[cfg(test)]
mod tests {
    use super::*;

    macro_rules! assert_token {
        ($token: expr, $ttype: ident, $value: expr) => (
            match $token {
                Token::$ttype(ref val) => assert_eq!(val, $value),
                _                      => panic!("invalid token {:?}", $token)
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

        assert_token!(tokens[0], Int, "10");
        assert_token!(tokens[1], Int, "10_5");
    }

    #[test]
    fn test_floats() {
        let tokens = tokenize!("10.5 10_5.5");

        assert_token!(tokens[0], Float, "10.5");
        assert_token!(tokens[1], Float, "10_5.5");
    }

    #[test]
    fn test_single_quote_strings() {
        let tokens = tokenize!("'foo' 'foo\\'bar'");

        assert_token!(tokens[0], String, "foo");
        assert_token!(tokens[1], String, "foo'bar");
    }

    #[test]
    fn test_double_quote_strings() {
        let tokens = tokenize!("\"hello\" \"hello\\\"world\"");

        assert_token!(tokens[0], String, "hello");
        assert_token!(tokens[1], String, "hello\"world");
    }

    #[test]
    fn test_identifiers() {
        let tokens = tokenize!("foo foö 한국어 _foo foo123 foo_bar");

        assert_token!(tokens[0], Identifier, "foo");
        assert_token!(tokens[1], Identifier, "foö");
        assert_token!(tokens[2], Identifier, "한국어");
        assert_token!(tokens[3], Identifier, "_foo");
        assert_token!(tokens[4], Identifier, "foo123");
        assert_token!(tokens[5], Identifier, "foo_bar");
    }

    #[test]
    fn test_constants() {
        let tokens = tokenize!("Foo Foö F한국어 Foo123 Foo_bar");

        assert_token!(tokens[0], Constant, "Foo");
        assert_token!(tokens[1], Constant, "Foö");
        assert_token!(tokens[2], Constant, "F한국어");
        assert_token!(tokens[3], Constant, "Foo123");
        assert_token!(tokens[4], Constant, "Foo_bar");
    }

    #[test]
    fn test_ivars() {
        let tokens = tokenize!("@foo @foö @한국어 @_foo @foo123 @foo_bar");

        assert_token!(tokens[0], InstanceVariable, "foo");
        assert_token!(tokens[1], InstanceVariable, "foö");
        assert_token!(tokens[2], InstanceVariable, "한국어");
        assert_token!(tokens[3], InstanceVariable, "_foo");
        assert_token!(tokens[4], InstanceVariable, "foo123");
        assert_token!(tokens[5], InstanceVariable, "foo_bar");
    }

    #[test]
    fn test_single_line_comment() {
        let tokens = tokenize!("# comment\nhello");

        assert_token!(tokens[0], Identifier, "hello");
    }

    #[test]
    fn test_docstring() {
        let tokens = tokenize!("/**/ /* foo */\n/* bar */ /* / */ /* * */");

        assert_token!(tokens[0], Docstring, "");
        assert_token!(tokens[1], Docstring, " foo ");
        assert_token!(tokens[2], Docstring, " bar ");
        assert_token!(tokens[3], Docstring, " / ");
        assert_token!(tokens[4], Docstring, " * ");
    }
}
