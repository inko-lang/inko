use lexer::{Lexer, Token, TokenType};

macro_rules! next_token {
    ($lexer: expr) => ({
        if let Some(token) = $lexer.lex() {
            token
        }
        else {
            return Err(ParserError::EndOfInput)
        }
    });
}

macro_rules! next_token_of_type {
    ($lexer: expr, $kind: ident) => ({
        let token = next_token!($lexer);

        match token.kind {
            TokenType::$kind => token,
            _                => return Err(ParserError::InvalidToken)
        }
    });
}

macro_rules! next_token_or_break {
    ($lexer: expr, $break_on: ident) => ({
        if let Some(token) = $lexer.lex() {
            match token.kind {
                TokenType::$break_on => break,
                _                    => token
            }
        }
        else {
            return Err(ParserError::EndOfInput);
        }
    });
}

macro_rules! comma_or_break {
    ($lexer: expr, $break_on: ident) => ({
        let token = next_token_or_break!($lexer, $break_on);

        match token.kind {
            TokenType::Comma => {},
            _                => return Err(ParserError::InvalidToken)
        };
    });
}

pub enum Node {
    Integer {
        value: isize,
        line: usize,
        column: usize
    },
    Float {
        value: f64,
        line: usize,
        column: usize
    },
    String {
        value: String,
        line: usize,
        column: usize
    },
    Expressions {
        children: Vec<Node>
    },
    Array {
        values: Vec<Node>,
        line: usize,
        column: usize
    },
    Hash {
        pairs: Vec<(Node, Node)>,
        line: usize,
        column: usize
    }
}

pub enum ParserError {
    EndOfInput,
    InvalidTokenValue,
    InvalidToken
}

pub type ParserResult = Result<Node, ParserError>;

pub fn parse(input: &str) -> ParserResult {
    let mut lexer = Lexer::new(input);

    parse_expressions(&mut lexer)
}

fn parse_expressions(lexer: &mut Lexer) -> ParserResult {
    let mut nodes = Vec::new();

    loop {
        if let Some(token) = lexer.lex() {
            nodes.push(try!(parse_expression(token, lexer)));
        }
        else {
            break;
        }
    }

    Ok(Node::Expressions { children: nodes })
}

fn parse_expression(token: Token, lexer: &mut Lexer) -> ParserResult {
    match token.kind {
        TokenType::Integer   => parse_integer(token),
        TokenType::Float     => parse_float(token),
        TokenType::String    => parse_string(token),
        TokenType::BrackOpen => parse_array(token, lexer),
        TokenType::CurlyOpen => parse_hash(token, lexer),
        _                    => Err(ParserError::InvalidToken)
    }
}

fn parse_integer(token: Token) -> ParserResult {
    let value = match token.value.parse::<isize>() {
        Ok(val) => val,
        Err(_)  => return Err(ParserError::InvalidTokenValue)
    };

    Ok(Node::Integer { value: value, line: token.line, column: token.column })
}

fn parse_float(token: Token) -> ParserResult {
    let value = match token.value.parse::<f64>() {
        Ok(val) => val,
        Err(_)  => return Err(ParserError::InvalidTokenValue)
    };

    Ok(Node::Float { value: value, line: token.line, column: token.column })
}

fn parse_string(token: Token) -> ParserResult {
    let value = token.value;

    Ok(Node::String { value: value, line: token.line, column: token.column })
}

fn parse_array(token: Token, lexer: &mut Lexer) -> ParserResult {
    let mut values = Vec::new();

    loop {
        let start = next_token_or_break!(lexer, BrackClose);

        values.push(try!(parse_expression(start, lexer)));

        comma_or_break!(lexer, BrackClose);
    }

    Ok(Node::Array { values: values, line: token.line, column: token.column })
}

fn parse_hash(token: Token, lexer: &mut Lexer) -> ParserResult {
    let mut pairs = Vec::new();

    loop {
        let kstart = next_token_or_break!(lexer, CurlyClose);
        let key    = try!(parse_expression(kstart, lexer));

        next_token_of_type!(lexer, Colon);

        let vstart = next_token!(lexer);
        let value  = try!(parse_expression(vstart, lexer));

        pairs.push((key, value));

        comma_or_break!(lexer, CurlyClose);
    }

    Ok(Node::Hash { pairs: pairs, line: token.line, column: token.column })
}
