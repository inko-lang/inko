#![allow(unused_parens)]
#![allow(unused_assignments)]
#![allow(non_upper_case_globals)]
#![allow(dead_code)]

use std::str;

macro_rules! to_string {
    ($data: ident, $start: ident, $end: ident) => (
        match str::from_utf8(&$data[$start .. $end]) {
            Ok(slice) => Ok(slice.to_string()),
            Err(_)    => Err(())
        }
    );
}

macro_rules! emit {
    ($token: ident, $data: ident, $start: ident, $end: ident, $callback: ident) => ({
        let value = try!(to_string!($data, $start, $end));
        let token = Token::$token(value);

        $callback(token);
    });
}

#[derive(Debug)]
pub enum Token {
    Int(String),
    Float(String)
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
    fn test_integer() {
        let tokens = tokenize!("10");

        assert_token!(tokens[0], Int, "10");
    }

    #[test]
    fn test_float() {
        let tokens = tokenize!("10.5");

        assert_token!(tokens[0], Float, "10.5");
    }
}
