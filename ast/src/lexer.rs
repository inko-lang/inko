//! Lexical analysis of Inko source code.
use crate::source_location::SourceLocation;
use unicode_segmentation::UnicodeSegmentation;

const NULL: u8 = 0;
const TAB: u8 = 9;
const NEWLINE: u8 = 10;
const CARRIAGE_RETURN: u8 = 13;
const ESCAPE: u8 = 27;
const SPACE: u8 = 32;
const EXCLAMATION: u8 = 33;
const DOUBLE_QUOTE: u8 = 34;
const HASH: u8 = 35;
const DOLLAR: u8 = 36;
const PERCENT: u8 = 37;
const AMPERSAND: u8 = 38;
const SINGLE_QUOTE: u8 = 39;
const PAREN_OPEN: u8 = 40;
const PAREN_CLOSE: u8 = 41;
const STAR: u8 = 42;
const PLUS: u8 = 43;
const COMMA: u8 = 44;
const MINUS: u8 = 45;
const DOT: u8 = 46;
const SLASH: u8 = 47;
const ZERO: u8 = 48;
const NINE: u8 = 57;
const COLON: u8 = 58;
const LESS: u8 = 60;
const EQUAL: u8 = 61;
const GREATER: u8 = 62;
const QUESTION: u8 = 63;
const AT_SIGN: u8 = 64;
const UPPER_A: u8 = 65;
const UPPER_E: u8 = 69;
const UPPER_F: u8 = 70;
const UPPER_X: u8 = 88;
const UPPER_Z: u8 = 90;
const BRACKET_OPEN: u8 = 91;
const BACKSLASH: u8 = 92;
const BRACKET_CLOSE: u8 = 93;
const CARET: u8 = 94;
const UNDERSCORE: u8 = 95;
const LOWER_A: u8 = 97;
const LOWER_E: u8 = 101;
const LOWER_F: u8 = 102;
const LOWER_N: u8 = 110;
const LOWER_R: u8 = 114;
const LOWER_T: u8 = 116;
const LOWER_U: u8 = 117;
const LOWER_X: u8 = 120;
const LOWER_Z: u8 = 122;
const CURLY_OPEN: u8 = 123;
const PIPE: u8 = 124;
const CURLY_CLOSE: u8 = 125;

/// The escape sequence literals supported by a single quoted string, and their
/// replacement bytes.
const SINGLE_ESCAPES: EscapeMap =
    EscapeMap::new().map(SINGLE_QUOTE, SINGLE_QUOTE).map(BACKSLASH, BACKSLASH);

/// The escape sequence literals supported by a double quoted string, and their
/// replacement bytes.
const DOUBLE_ESCAPES: EscapeMap = EscapeMap::new()
    .map(DOUBLE_QUOTE, DOUBLE_QUOTE)
    .map(SINGLE_QUOTE, SINGLE_QUOTE)
    .map(ZERO, NULL)
    .map(BACKSLASH, BACKSLASH)
    .map(LOWER_E, ESCAPE)
    .map(LOWER_N, NEWLINE)
    .map(LOWER_R, CARRIAGE_RETURN)
    .map(LOWER_T, TAB)
    .map(CURLY_OPEN, CURLY_OPEN);

#[derive(PartialEq, Eq, Debug, Copy, Clone)]
pub enum TokenKind {
    Add,
    AddAssign,
    And,
    Arrow,
    As,
    Assign,
    Async,
    BitAnd,
    BitAndAssign,
    BitOr,
    BitOrAssign,
    BitXor,
    BitXorAssign,
    BracketClose,
    BracketOpen,
    Break,
    Builtin,
    Case,
    Class,
    Colon,
    Comma,
    Comment,
    Constant,
    CurlyClose,
    CurlyOpen,
    Div,
    DivAssign,
    Dot,
    DoubleArrow,
    DoubleStringClose,
    DoubleStringOpen,
    Else,
    Enum,
    Eq,
    False,
    Field,
    Float,
    Fn,
    For,
    Ge,
    Gt,
    Identifier,
    If,
    Implement,
    Import,
    Integer,
    Invalid,
    InvalidUnicodeEscape,
    Le,
    Let,
    Loop,
    Lt,
    Match,
    Mod,
    ModAssign,
    Move,
    Mul,
    MulAssign,
    Mut,
    Ne,
    Next,
    Nil,
    Null,
    Or,
    ParenClose,
    ParenOpen,
    Pow,
    PowAssign,
    Pub,
    Recover,
    Ref,
    Replace,
    Return,
    SelfObject,
    Shl,
    ShlAssign,
    Shr,
    ShrAssign,
    SingleStringClose,
    SingleStringOpen,
    Static,
    StringExprClose,
    StringExprOpen,
    StringText,
    Sub,
    SubAssign,
    Throw,
    Trait,
    True,
    Try,
    Uni,
    UnicodeEscape,
    UnsignedShr,
    UnsignedShrAssign,
    While,
    Whitespace,
    Extern,
}

impl TokenKind {
    pub fn description(&self) -> &str {
        match self {
            TokenKind::Add => "a '+'",
            TokenKind::AddAssign => "a '+='",
            TokenKind::And => "the 'and' keyword",
            TokenKind::Arrow => "a '->'",
            TokenKind::As => "the 'as' keyword",
            TokenKind::Assign => "a '='",
            TokenKind::Async => "the 'async' keyword",
            TokenKind::BitAnd => "a '&'",
            TokenKind::BitAndAssign => "a '&='",
            TokenKind::BitOr => "a '|'",
            TokenKind::BitOrAssign => "a '|='",
            TokenKind::BitXor => "a '^'",
            TokenKind::BitXorAssign => "a '^='",
            TokenKind::BracketClose => "a ']'",
            TokenKind::BracketOpen => "an '['",
            TokenKind::Break => "the 'break' keyword",
            TokenKind::Class => "the 'class' keyword",
            TokenKind::Colon => "a ':'",
            TokenKind::Comma => "a ','",
            TokenKind::Comment => "a comment",
            TokenKind::Constant => "a constant",
            TokenKind::CurlyClose => "a '}'",
            TokenKind::CurlyOpen => "a '{'",
            TokenKind::Div => "a '/'",
            TokenKind::DivAssign => "a '/='",
            TokenKind::Dot => "a '.'",
            TokenKind::DoubleArrow => "a '=>'",
            TokenKind::DoubleStringClose => "a '\"'",
            TokenKind::DoubleStringOpen => "a '\"'",
            TokenKind::Else => "the 'else' keyword",
            TokenKind::Eq => "a '=='",
            TokenKind::Builtin => "the 'builtin' keyword",
            TokenKind::Field => "a field",
            TokenKind::Float => "a float",
            TokenKind::Fn => "the 'fn' keyword",
            TokenKind::For => "the 'for' keyword",
            TokenKind::Gt => "a '>'",
            TokenKind::Ge => "a '>='",
            TokenKind::Identifier => "an identifier",
            TokenKind::If => "the 'if' keyword",
            TokenKind::Implement => "the 'impl' keyword",
            TokenKind::Import => "the 'import' keyword",
            TokenKind::Integer => "an integer",
            TokenKind::Invalid => "an invalid token",
            TokenKind::InvalidUnicodeEscape => {
                "an invalid Unicode escape sequence"
            }
            TokenKind::Lt => "a '<'",
            TokenKind::Le => "a '<='",
            TokenKind::Let => "the 'let' keyword",
            TokenKind::Loop => "the 'loop' keyword",
            TokenKind::Match => "the 'match' keyword",
            TokenKind::Mod => "a '%'",
            TokenKind::ModAssign => "a '%='",
            TokenKind::Mul => "a '*'",
            TokenKind::MulAssign => "a '*='",
            TokenKind::Next => "the 'next' keyword",
            TokenKind::Ne => "a '!='",
            TokenKind::Null => "the end of the input",
            TokenKind::Or => "the 'or' keyword",
            TokenKind::ParenClose => "a closing parenthesis",
            TokenKind::ParenOpen => "an opening parenthesis",
            TokenKind::Pow => "a '**'",
            TokenKind::PowAssign => "a '**='",
            TokenKind::Ref => "the 'ref' keyword",
            TokenKind::Return => "the 'return' keyword",
            TokenKind::SelfObject => "the 'self' keyword",
            TokenKind::Shl => "a '<<'",
            TokenKind::ShlAssign => "a '<<='",
            TokenKind::Shr => "a '>>'",
            TokenKind::ShrAssign => "a '>>='",
            TokenKind::UnsignedShr => "a '>>>'",
            TokenKind::UnsignedShrAssign => "a '>>>='",
            TokenKind::SingleStringClose => "a '''",
            TokenKind::SingleStringOpen => "a '''",
            TokenKind::Static => "the 'static' keyword",
            TokenKind::StringExprClose => "a closing curly brace",
            TokenKind::StringExprOpen => "an opening curly brace",
            TokenKind::StringText => "the text of a string",
            TokenKind::Sub => "a '-'",
            TokenKind::SubAssign => "a '-='",
            TokenKind::Throw => "the 'throw' keyword",
            TokenKind::Trait => "the 'trait' keyword",
            TokenKind::Try => "the 'try' keyword",
            TokenKind::UnicodeEscape => "an Unicode escape sequence",
            TokenKind::While => "the 'while' keyword",
            TokenKind::Whitespace => "whitespace",
            TokenKind::Mut => "the 'mut' keyword",
            TokenKind::Uni => "the 'uni' keyword",
            TokenKind::Pub => "the 'pub' keyword",
            TokenKind::Move => "the 'move' keyword",
            TokenKind::True => "the 'true' keyword",
            TokenKind::False => "the 'false' keyword",
            TokenKind::Case => "the 'case' keyword",
            TokenKind::Enum => "the 'enum' keyword",
            TokenKind::Recover => "the 'recover' keyword",
            TokenKind::Nil => "the 'nil' keyword",
            TokenKind::Replace => "a '=:'",
            TokenKind::Extern => "the 'extern' keyword",
        }
    }
}

#[derive(PartialEq, Eq, Debug)]
pub struct Token {
    pub kind: TokenKind,
    pub value: String,
    pub location: SourceLocation,
}

impl Token {
    fn new(kind: TokenKind, value: String, location: SourceLocation) -> Self {
        Self { kind, value, location }
    }

    /// Returns a token signalling unexpected input. The token contains the
    /// invalid character.
    fn invalid(value: String, location: SourceLocation) -> Self {
        Self::new(TokenKind::Invalid, value, location)
    }

    /// Returns a token that signals the end of the input stream. We use null
    /// tokens so we don't need to wrap/unwrap every token using an Option type.
    fn null(location: SourceLocation) -> Self {
        Self::new(TokenKind::Null, String::new(), location)
    }

    pub fn is_keyword(&self) -> bool {
        matches!(
            self.kind,
            TokenKind::And
                | TokenKind::As
                | TokenKind::Async
                | TokenKind::Break
                | TokenKind::Class
                | TokenKind::Else
                | TokenKind::Builtin
                | TokenKind::Fn
                | TokenKind::For
                | TokenKind::If
                | TokenKind::Implement
                | TokenKind::Import
                | TokenKind::Let
                | TokenKind::Loop
                | TokenKind::Match
                | TokenKind::Next
                | TokenKind::Or
                | TokenKind::Ref
                | TokenKind::Return
                | TokenKind::SelfObject
                | TokenKind::Static
                | TokenKind::Throw
                | TokenKind::Trait
                | TokenKind::Try
                | TokenKind::While
                | TokenKind::Mut
                | TokenKind::Recover
                | TokenKind::Uni
                | TokenKind::Pub
                | TokenKind::Move
                | TokenKind::True
                | TokenKind::Nil
                | TokenKind::False
                | TokenKind::Case
                | TokenKind::Enum
                | TokenKind::Extern
        )
    }

    pub fn is_operator(&self) -> bool {
        matches!(
            self.kind,
            TokenKind::Add
                | TokenKind::Sub
                | TokenKind::Div
                | TokenKind::Mul
                | TokenKind::Mod
                | TokenKind::Pow
                | TokenKind::BitAnd
                | TokenKind::BitOr
                | TokenKind::BitXor
                | TokenKind::Shl
                | TokenKind::Shr
                | TokenKind::UnsignedShr
                | TokenKind::Lt
                | TokenKind::Le
                | TokenKind::Gt
                | TokenKind::Ge
                | TokenKind::Eq
                | TokenKind::Ne
        )
    }

    pub fn same_line_as(&self, token: &Token) -> bool {
        self.location.line_range.start() == token.location.line_range.start()
    }
}

/// Mapping of string escape sequences to their replacement bytes.
struct EscapeMap {
    mapping: [Option<u8>; 128],
}

impl EscapeMap {
    const fn new() -> Self {
        Self { mapping: [None; 128] }
    }

    const fn map(mut self, byte: u8, to: u8) -> Self {
        self.mapping[byte as usize] = Some(to);
        self
    }

    const fn get(&self, index: u8) -> Option<u8> {
        let idx = index as usize;

        if idx < self.mapping.len() {
            return self.mapping[idx];
        }

        None
    }
}

#[derive(Copy, Clone)]
enum State {
    Default,
    SingleString,
    DoubleString,
    EscapedWhitespace,
}

/// A lexer for Inko source code.
pub struct Lexer {
    /// The stream of bytes to process.
    input: Vec<u8>,

    /// The maximum position in the input stream.
    max_position: usize,

    /// The current position in the input stream.
    position: usize,

    /// The number of opening curly braces that have yet to be closed.
    curly_braces: usize,

    // The stack of curly brace counts to use for determining when a string
    // expression should be closed.
    curly_brace_stack: Vec<usize>,

    /// The stack of lexing states.
    states: Vec<State>,

    /// The current line number.
    line: usize,

    /// The current (starting) column number.
    column: usize,
}

impl Lexer {
    pub fn new(input: Vec<u8>) -> Self {
        let max = input.len();
        Self {
            input,
            max_position: max,
            position: 0,
            curly_braces: 0,
            curly_brace_stack: Vec::new(),
            states: vec![State::Default],
            line: 1,
            column: 1,
        }
    }

    pub fn start_location(&self) -> SourceLocation {
        SourceLocation::new(self.line..=self.line, self.column..=self.column)
    }

    pub fn next_token(&mut self) -> Token {
        match self.states.last().cloned() {
            Some(State::SingleString) => self.next_single_string_token(),
            Some(State::DoubleString) => self.next_double_string_token(),
            Some(State::EscapedWhitespace) => {
                self.consume_escaped_whitespace();
                self.next_token()
            }
            _ => self.next_regular_token(),
        }
    }

    fn source_location(
        &self,
        start_line: usize,
        start_column: usize,
    ) -> SourceLocation {
        SourceLocation::new(
            start_line..=self.line,
            // The end column points to whatever comes _after_ the last
            // processed character. This means the end column is one column
            // earlier.
            start_column..=(self.column - 1),
        )
    }

    fn current_byte(&self) -> u8 {
        if self.has_next() {
            self.input[self.position]
        } else {
            0
        }
    }

    fn next_byte(&self) -> u8 {
        self.peek(1)
    }

    fn peek(&self, offset: usize) -> u8 {
        let index = self.position + offset;

        if index < self.max_position {
            self.input[index]
        } else {
            0
        }
    }

    fn advance_line(&mut self) {
        self.position += 1;
        self.column = 1;
        self.line += 1;
    }

    fn advance_column(&mut self, value: &str) {
        self.column += value.graphemes(true).count();
    }

    fn advance_char(&mut self) {
        self.column += 1;
        self.position += 1;
    }

    fn has_next(&self) -> bool {
        self.position < self.max_position
    }

    fn next_double_string_token(&mut self) -> Token {
        match self.current_byte() {
            DOUBLE_QUOTE => {
                self.states.pop();
                self.single_character_token(TokenKind::DoubleStringClose)
            }
            BACKSLASH if self.next_is_unicode_escape() => {
                self.unicode_escape_token()
            }
            CURLY_OPEN => self.double_string_expression_open(),
            _ if self.has_next() => self.double_string_text(),
            _ => self.null(),
        }
    }

    fn next_single_string_token(&mut self) -> Token {
        match self.current_byte() {
            SINGLE_QUOTE => {
                self.states.pop();
                self.single_character_token(TokenKind::SingleStringClose)
            }
            _ if self.has_next() => self.single_string_text(),
            _ => self.null(),
        }
    }

    fn unicode_escape_token(&mut self) -> Token {
        let line = self.line;
        let column = self.column;
        let mut buffer = Vec::new();
        let mut closed = false;

        // Advance three characters for the `\u{`.
        self.position += 3;
        self.column += 3;

        while self.has_next() {
            let byte = self.current_byte();

            if byte == CURLY_CLOSE {
                closed = true;

                self.advance_char();
                break;
            }

            if byte == DOUBLE_QUOTE {
                break;
            }

            self.position += 1;
            buffer.push(byte);
        }

        let mut kind = TokenKind::InvalidUnicodeEscape;
        let mut value = String::from_utf8_lossy(&buffer).into_owned();

        self.advance_column(&value);

        let location = self.source_location(line, column);

        if closed && !value.is_empty() && value.len() <= 6 {
            if let Some(parsed) = u32::from_str_radix(&value, 16)
                .ok()
                .and_then(char::from_u32)
                .map(|chr| chr.to_string())
            {
                kind = TokenKind::UnicodeEscape;
                value = parsed;
            }
        }

        Token::new(kind, value, location)
    }

    fn next_regular_token(&mut self) -> Token {
        match self.current_byte() {
            ZERO..=NINE => self.number(false),
            AT_SIGN => self.field(),
            HASH => self.comment(),
            CURLY_OPEN => self.curly_open(),
            CURLY_CLOSE => self.curly_close(),
            PAREN_OPEN => self.paren_open(),
            PAREN_CLOSE => self.paren_close(),
            SINGLE_QUOTE => self.single_quote(),
            DOUBLE_QUOTE => self.double_quote(),
            COLON => self.colon(),
            PERCENT => self.percent(),
            SLASH => self.slash(),
            CARET => self.caret(),
            AMPERSAND => self.ampersand(),
            PIPE => self.pipe(),
            STAR => self.star(),
            MINUS => self.minus(),
            PLUS => self.plus(),
            EQUAL => self.equal(),
            LESS => self.less(),
            GREATER => self.greater(),
            BRACKET_OPEN => self.bracket_open(),
            BRACKET_CLOSE => self.bracket_close(),
            EXCLAMATION => self.exclamation(),
            DOT => self.dot(),
            COMMA => self.comma(),
            UNDERSCORE => self.underscore(),
            LOWER_A..=LOWER_Z => self.identifier_or_keyword(self.position),
            UPPER_A..=UPPER_Z => self.constant(self.position),
            SPACE | TAB | CARRIAGE_RETURN | NEWLINE => self.whitespace(),
            _ => {
                if self.has_next() {
                    self.invalid(self.position, self.position + 1)
                } else {
                    self.null()
                }
            }
        }
    }

    fn whitespace(&mut self) -> Token {
        let start = self.position;
        let line = self.line;
        let column = self.column;
        let mut new_line = false;

        while self.has_next() {
            match self.current_byte() {
                SPACE | TAB | CARRIAGE_RETURN => self.advance_char(),
                NEWLINE => {
                    new_line = true;

                    self.advance_char();
                    break;
                }
                _ => break,
            }
        }

        let value = self.slice_string(start, self.position);
        let location = self.source_location(line, column);

        if new_line {
            self.column = 1;
            self.line += 1;
        }

        Token::new(TokenKind::Whitespace, value, location)
    }

    fn number(&mut self, skip_first: bool) -> Token {
        let start = self.position;
        let line = self.line;

        if skip_first {
            self.position += 1;
        }

        let first = self.current_byte();
        let second = self.next_byte();
        let mut kind = TokenKind::Integer;

        if first == ZERO && (second == LOWER_X || second == UPPER_X) {
            // Advance 2 for "0x"
            self.position += 2;

            while let ZERO..=NINE
            | LOWER_A..=LOWER_F
            | UPPER_A..=UPPER_F
            | UNDERSCORE = self.current_byte()
            {
                self.position += 1;
            }

            return self.token(kind, start, line);
        }

        loop {
            match self.current_byte() {
                ZERO..=NINE | UNDERSCORE => {}
                LOWER_E | UPPER_E => match self.next_byte() {
                    // 10e5, 10E5, etc
                    ZERO..=NINE => {
                        kind = TokenKind::Float;
                    }
                    // 10e+5, 10e-5, etc
                    PLUS | MINUS if (ZERO..=NINE).contains(&self.peek(2)) => {
                        self.position += 1;
                        kind = TokenKind::Float;
                    }
                    _ => break,
                },
                DOT if (ZERO..=NINE).contains(&self.next_byte()) => {
                    kind = TokenKind::Float;
                }
                _ => break,
            }

            self.position += 1;
        }

        self.token(kind, start, line)
    }

    fn field(&mut self) -> Token {
        let column = self.column;
        let line = self.line;

        self.advance_char();

        let start = self.position;

        self.advance_identifier_bytes();
        self.token_with_column(TokenKind::Field, start, line, column)
    }

    fn comment(&mut self) -> Token {
        let column = self.column;
        let line = self.line;

        self.advance_char();

        // The first space in a comment is ignored, so that for comment `# foo`
        // the text is `foo` and not ` foo`.
        if self.current_byte() == SPACE {
            self.advance_char();
        }

        let start = self.position;

        while self.has_next() && self.current_byte() != NEWLINE {
            self.position += 1;
        }

        let comment =
            self.token_with_column(TokenKind::Comment, start, line, column);

        self.advance_line();
        comment
    }

    fn curly_open(&mut self) -> Token {
        self.curly_braces += 1;

        self.single_character_token(TokenKind::CurlyOpen)
    }

    fn curly_close(&mut self) -> Token {
        if self.curly_braces > 0 {
            self.curly_braces -= 1;
        }

        let count = self.curly_brace_stack.last().cloned();

        if count == Some(self.curly_braces) {
            self.curly_brace_stack.pop();
            self.states.pop();

            self.single_character_token(TokenKind::StringExprClose)
        } else {
            self.single_character_token(TokenKind::CurlyClose)
        }
    }

    fn paren_open(&mut self) -> Token {
        self.single_character_token(TokenKind::ParenOpen)
    }

    fn paren_close(&mut self) -> Token {
        self.single_character_token(TokenKind::ParenClose)
    }

    fn single_quote(&mut self) -> Token {
        self.states.push(State::SingleString);
        self.single_character_token(TokenKind::SingleStringOpen)
    }

    fn double_quote(&mut self) -> Token {
        self.states.push(State::DoubleString);
        self.single_character_token(TokenKind::DoubleStringOpen)
    }

    fn colon(&mut self) -> Token {
        let start = self.position;
        let line = self.line;
        let (incr, kind) = match self.next_byte() {
            EQUAL => (2, TokenKind::Replace),
            _ => (1, TokenKind::Colon),
        };

        self.position += incr;

        self.token(kind, start, line)
    }

    fn percent(&mut self) -> Token {
        self.operator(TokenKind::Mod, TokenKind::ModAssign, self.position)
    }

    fn slash(&mut self) -> Token {
        self.operator(TokenKind::Div, TokenKind::DivAssign, self.position)
    }

    fn caret(&mut self) -> Token {
        self.operator(TokenKind::BitXor, TokenKind::BitXorAssign, self.position)
    }

    fn ampersand(&mut self) -> Token {
        self.operator(TokenKind::BitAnd, TokenKind::BitAndAssign, self.position)
    }

    fn pipe(&mut self) -> Token {
        self.operator(TokenKind::BitOr, TokenKind::BitOrAssign, self.position)
    }

    fn star(&mut self) -> Token {
        if self.next_byte() == STAR {
            return self.double_operator(TokenKind::Pow, TokenKind::PowAssign);
        }

        self.operator(TokenKind::Mul, TokenKind::MulAssign, self.position)
    }

    fn minus(&mut self) -> Token {
        match self.next_byte() {
            ZERO..=NINE => self.number(true),
            GREATER => self.arrow(),
            _ => self.operator(
                TokenKind::Sub,
                TokenKind::SubAssign,
                self.position,
            ),
        }
    }

    fn plus(&mut self) -> Token {
        self.operator(TokenKind::Add, TokenKind::AddAssign, self.position)
    }

    fn arrow(&mut self) -> Token {
        let start = self.position;

        self.position += 2;
        self.token(TokenKind::Arrow, start, self.line)
    }

    fn equal(&mut self) -> Token {
        let start = self.position;
        let (incr, kind) = match self.next_byte() {
            EQUAL => (2, TokenKind::Eq),
            GREATER => (2, TokenKind::DoubleArrow),
            COLON => (2, TokenKind::Replace),
            _ => (1, TokenKind::Assign),
        };

        self.position += incr;
        self.token(kind, start, self.line)
    }

    fn less(&mut self) -> Token {
        if self.next_byte() == LESS {
            return self.double_operator(TokenKind::Shl, TokenKind::ShlAssign);
        }

        self.operator(TokenKind::Lt, TokenKind::Le, self.position)
    }

    fn greater(&mut self) -> Token {
        if self.next_byte() == GREATER {
            return if self.peek(2) == GREATER {
                self.triple_operator(
                    TokenKind::UnsignedShr,
                    TokenKind::UnsignedShrAssign,
                )
            } else {
                self.double_operator(TokenKind::Shr, TokenKind::ShrAssign)
            };
        }

        self.operator(TokenKind::Gt, TokenKind::Ge, self.position)
    }

    fn bracket_open(&mut self) -> Token {
        self.single_character_token(TokenKind::BracketOpen)
    }

    fn bracket_close(&mut self) -> Token {
        self.single_character_token(TokenKind::BracketClose)
    }

    fn exclamation(&mut self) -> Token {
        match self.next_byte() {
            EQUAL => {
                let start = self.position;

                self.position += 2;
                self.token(TokenKind::Ne, start, self.line)
            }
            _ => self.invalid(self.position, self.position + 1),
        }
    }

    fn dot(&mut self) -> Token {
        let start = self.position;
        let line = self.line;

        self.position += 1;

        self.token(TokenKind::Dot, start, line)
    }

    fn comma(&mut self) -> Token {
        self.single_character_token(TokenKind::Comma)
    }

    fn identifier_or_keyword(&mut self, start: usize) -> Token {
        let column = self.column;

        self.advance_identifier_bytes();

        let value = self.slice_string(start, self.position);

        // We use this approach so that:
        //
        // 1. We can avoid the worst case of performing a linear search through
        //    all keywords and not find a match (= every regular identifier).
        // 2. Because of that it's faster than just a regular match on `str`
        //    values.
        // 3. It's easy to port to Inko's self-hosting compiler (unlike for
        //    example a perfect hashing solution).
        let kind = match value.len() {
            2 => match value.as_str() {
                "as" => TokenKind::As,
                "fn" => TokenKind::Fn,
                "if" => TokenKind::If,
                "or" => TokenKind::Or,
                _ => TokenKind::Identifier,
            },
            3 => match value.as_str() {
                "and" => TokenKind::And,
                "for" => TokenKind::For,
                "let" => TokenKind::Let,
                "ref" => TokenKind::Ref,
                "try" => TokenKind::Try,
                "mut" => TokenKind::Mut,
                "uni" => TokenKind::Uni,
                "pub" => TokenKind::Pub,
                "nil" => TokenKind::Nil,
                _ => TokenKind::Identifier,
            },
            4 => match value.as_str() {
                "else" => TokenKind::Else,
                "impl" => TokenKind::Implement,
                "loop" => TokenKind::Loop,
                "next" => TokenKind::Next,
                "self" => TokenKind::SelfObject,
                "move" => TokenKind::Move,
                "true" => TokenKind::True,
                "case" => TokenKind::Case,
                "enum" => TokenKind::Enum,
                _ => TokenKind::Identifier,
            },
            5 => match value.as_str() {
                "class" => TokenKind::Class,
                "async" => TokenKind::Async,
                "break" => TokenKind::Break,
                "match" => TokenKind::Match,
                "throw" => TokenKind::Throw,
                "trait" => TokenKind::Trait,
                "while" => TokenKind::While,
                "false" => TokenKind::False,
                _ => TokenKind::Identifier,
            },
            6 => match value.as_str() {
                "import" => TokenKind::Import,
                "return" => TokenKind::Return,
                "static" => TokenKind::Static,
                "extern" => TokenKind::Extern,
                _ => TokenKind::Identifier,
            },
            7 => match value.as_str() {
                "builtin" => TokenKind::Builtin,
                "recover" => TokenKind::Recover,
                _ => TokenKind::Identifier,
            },
            _ => TokenKind::Identifier,
        };

        self.advance_column(&value);

        let location = self.source_location(self.line, column);

        Token::new(kind, value, location)
    }

    fn constant(&mut self, start: usize) -> Token {
        let column = self.column;

        self.advance_identifier_bytes();

        let value = self.slice_string(start, self.position);

        self.advance_column(&value);

        let location = self.source_location(self.line, column);

        Token::new(TokenKind::Constant, value, location)
    }

    fn underscore(&mut self) -> Token {
        let start = self.position;

        while self.current_byte() == UNDERSCORE {
            self.position += 1;
        }

        match self.current_byte() {
            UPPER_A..=UPPER_Z => self.constant(start),
            LOWER_A..=LOWER_Z => self.identifier_or_keyword(start),
            ZERO..=NINE => self.identifier_or_keyword(start),
            _ => self.identifier_or_keyword(start),
        }
    }

    fn single_string_text(&mut self) -> Token {
        let kind = TokenKind::StringText;
        let mut buffer = Vec::new();
        let mut new_line = false;
        let line = self.line;
        let column = self.column;

        while self.has_next() {
            match self.current_byte() {
                BACKSLASH => {
                    let next = self.next_byte();

                    if self.enter_escaped_whitespace(next) {
                        break;
                    }

                    if self.replace_escape_sequence(
                        &mut buffer,
                        next,
                        &SINGLE_ESCAPES,
                    ) {
                        continue;
                    }

                    buffer.push(BACKSLASH);

                    self.position += 1;
                }
                NEWLINE => {
                    new_line = true;

                    buffer.push(NEWLINE);
                    break;
                }
                SINGLE_QUOTE => {
                    break;
                }
                byte => {
                    buffer.push(byte);

                    self.position += 1;
                }
            }
        }

        self.string_text_token(kind, buffer, line, column, new_line)
    }

    fn double_string_text(&mut self) -> Token {
        let kind = TokenKind::StringText;
        let mut buffer = Vec::new();
        let mut new_line = false;
        let line = self.line;
        let column = self.column;

        while self.has_next() {
            match self.current_byte() {
                BACKSLASH => {
                    if self.next_is_unicode_escape() {
                        break;
                    }

                    let next = self.next_byte();

                    if self.enter_escaped_whitespace(next) {
                        break;
                    }

                    if self.replace_escape_sequence(
                        &mut buffer,
                        next,
                        &DOUBLE_ESCAPES,
                    ) {
                        continue;
                    }

                    buffer.push(BACKSLASH);

                    self.position += 1;
                }
                NEWLINE => {
                    new_line = true;

                    buffer.push(NEWLINE);
                    break;
                }
                DOUBLE_QUOTE | CURLY_OPEN => {
                    break;
                }
                byte => {
                    buffer.push(byte);

                    self.position += 1;
                }
            }
        }

        self.string_text_token(kind, buffer, line, column, new_line)
    }

    fn double_string_expression_open(&mut self) -> Token {
        self.states.push(State::Default);
        self.curly_brace_stack.push(self.curly_braces);

        self.curly_braces += 1;

        self.single_character_token(TokenKind::StringExprOpen)
    }

    fn enter_escaped_whitespace(&mut self, byte: u8) -> bool {
        if !self.is_whitespace(byte) {
            return false;
        }

        self.advance_char();
        self.states.push(State::EscapedWhitespace);
        true
    }

    fn consume_escaped_whitespace(&mut self) {
        loop {
            match self.current_byte() {
                SPACE | TAB | CARRIAGE_RETURN => self.advance_char(),
                NEWLINE => self.advance_line(),
                _ => break,
            }
        }

        self.states.pop();
    }

    fn replace_escape_sequence(
        &mut self,
        buffer: &mut Vec<u8>,
        byte: u8,
        replacements: &EscapeMap,
    ) -> bool {
        if let Some(replace) = replacements.get(byte) {
            buffer.push(replace);

            // The replacement is included in the buffer, meaning we'll also
            // include it for advancing column numbers.  As such we only need to
            // advance for the backslash here.
            self.column += 1;
            self.position += 2;

            true
        } else {
            false
        }
    }

    fn string_text_token(
        &mut self,
        kind: TokenKind,
        buffer: Vec<u8>,
        line: usize,
        column: usize,
        new_line: bool,
    ) -> Token {
        let value = String::from_utf8_lossy(&buffer).into_owned();

        if !value.is_empty() {
            self.column += value.graphemes(true).count();
        }

        let location = self.source_location(line, column);

        if new_line {
            self.advance_line();
        }

        Token::new(kind, value, location)
    }

    fn advance_identifier_bytes(&mut self) {
        loop {
            match self.current_byte() {
                ZERO..=NINE
                | LOWER_A..=LOWER_Z
                | UPPER_A..=UPPER_Z
                | UNDERSCORE
                | DOLLAR => self.position += 1,
                QUESTION => {
                    self.position += 1;
                    break;
                }
                _ => break,
            }
        }
    }

    fn is_whitespace(&self, byte: u8) -> bool {
        matches!(byte, SPACE | TAB | CARRIAGE_RETURN | NEWLINE)
    }

    fn next_is_unicode_escape(&self) -> bool {
        self.next_byte() == LOWER_U && self.peek(2) == CURLY_OPEN
    }

    fn triple_operator(
        &mut self,
        kind: TokenKind,
        assign_kind: TokenKind,
    ) -> Token {
        let start = self.position;

        self.position += 2;

        self.operator(kind, assign_kind, start)
    }

    fn double_operator(
        &mut self,
        kind: TokenKind,
        assign_kind: TokenKind,
    ) -> Token {
        let start = self.position;

        self.position += 1;

        self.operator(kind, assign_kind, start)
    }

    fn operator(
        &mut self,
        kind: TokenKind,
        assign_kind: TokenKind,
        start: usize,
    ) -> Token {
        let mut token_kind = kind;
        let mut incr = 1;

        if self.next_byte() == EQUAL {
            token_kind = assign_kind;
            incr = 2;
        }

        self.position += incr;

        let value = self.slice_string(start, self.position);
        let line = self.line;
        let column = self.column;

        self.advance_column(&value);
        Token::new(token_kind, value, self.source_location(line, column))
    }

    fn token_with_column(
        &mut self,
        kind: TokenKind,
        start: usize,
        line: usize,
        column: usize,
    ) -> Token {
        let value = self.slice_string(start, self.position);

        self.advance_column(&value);

        let location = self.source_location(line, column);

        Token::new(kind, value, location)
    }

    fn token(&mut self, kind: TokenKind, start: usize, line: usize) -> Token {
        self.token_with_column(kind, start, line, self.column)
    }

    fn single_character_token(&mut self, kind: TokenKind) -> Token {
        let start = self.position;
        let line = self.line;

        self.position += 1;

        self.token(kind, start, line)
    }

    fn slice_string(&mut self, start: usize, stop: usize) -> String {
        String::from_utf8_lossy(&self.input[start..stop]).into_owned()
    }

    fn invalid(&mut self, start: usize, stop: usize) -> Token {
        let column = self.column;
        let value = self.slice_string(start, stop);

        self.advance_column(&value);

        let location = self.source_location(self.line, column);

        // When we run into invalid input we want to immediately stop processing
        // any further input.
        self.position = self.max_position;

        Token::invalid(value, location)
    }

    fn null(&self) -> Token {
        // When we encounter the end of the input, we want the location to point
        // to the last column that came before it. This way any errors are
        // reported within the bounds of the column range.
        let lines = self.line..=self.line;
        let location = if self.column == 1 {
            SourceLocation::new(lines, 1..=1)
        } else {
            let column = self.column - 1;

            SourceLocation::new(lines, column..=column)
        };

        Token::null(location)
    }
}

#[cfg(test)]
mod tests {
    use super::TokenKind::*;
    use super::*;
    use std::ops::RangeInclusive;

    fn lexer(input: &str) -> Lexer {
        Lexer::new(Vec::from(input))
    }

    fn location(
        line_range: RangeInclusive<usize>,
        column_range: RangeInclusive<usize>,
    ) -> SourceLocation {
        SourceLocation::new(line_range, column_range)
    }

    fn tok(
        kind: TokenKind,
        value: &str,
        line_range: RangeInclusive<usize>,
        column_range: RangeInclusive<usize>,
    ) -> Token {
        Token::new(kind, value.to_string(), location(line_range, column_range))
    }

    // We use a macro here so any test failures report the line that called the
    // macro, not the line inside a function that failed. This makes debugging
    // easier.
    macro_rules! assert_token {
        (
            $input: expr,
            $kind: expr,
            $value: expr,
            $lines: expr,
            $columns: expr
        ) => {{
            let mut lexer = lexer($input);
            let token = lexer.next_token();

            assert_eq!(token, tok($kind, $value, $lines, $columns))
        }};
    }

    macro_rules! assert_tokens {
        (
            $input: expr,
            $(
                $token: expr
            ),+
        ) => {{
            let mut lexer = lexer($input);
            let mut tokens = Vec::new();
            let mut token = lexer.next_token();

            while token.kind != TokenKind::Null {
                tokens.push(token);

                token = lexer.next_token();
            }

            assert_eq!(tokens, vec![$( $token, )+]);
        }};
    }

    #[test]
    fn test_token_is_keyword() {
        assert!(tok(TokenKind::As, "", 1..=1, 1..=1).is_keyword());
        assert!(tok(TokenKind::Async, "", 1..=1, 1..=1).is_keyword());
        assert!(tok(TokenKind::Break, "", 1..=1, 1..=1).is_keyword());
        assert!(tok(TokenKind::Builtin, "", 1..=1, 1..=1).is_keyword());
        assert!(tok(TokenKind::Case, "", 1..=1, 1..=1).is_keyword());
        assert!(tok(TokenKind::Class, "", 1..=1, 1..=1).is_keyword());
        assert!(tok(TokenKind::Else, "", 1..=1, 1..=1).is_keyword());
        assert!(tok(TokenKind::Enum, "", 1..=1, 1..=1).is_keyword());
        assert!(tok(TokenKind::False, "", 1..=1, 1..=1).is_keyword());
        assert!(tok(TokenKind::Fn, "", 1..=1, 1..=1).is_keyword());
        assert!(tok(TokenKind::For, "", 1..=1, 1..=1).is_keyword());
        assert!(tok(TokenKind::If, "", 1..=1, 1..=1).is_keyword());
        assert!(tok(TokenKind::Implement, "", 1..=1, 1..=1).is_keyword());
        assert!(tok(TokenKind::Import, "", 1..=1, 1..=1).is_keyword());
        assert!(tok(TokenKind::Let, "", 1..=1, 1..=1).is_keyword());
        assert!(tok(TokenKind::Loop, "", 1..=1, 1..=1).is_keyword());
        assert!(tok(TokenKind::Match, "", 1..=1, 1..=1).is_keyword());
        assert!(tok(TokenKind::Move, "", 1..=1, 1..=1).is_keyword());
        assert!(tok(TokenKind::Mut, "", 1..=1, 1..=1).is_keyword());
        assert!(tok(TokenKind::Uni, "", 1..=1, 1..=1).is_keyword());
        assert!(tok(TokenKind::Pub, "", 1..=1, 1..=1).is_keyword());
        assert!(tok(TokenKind::Next, "", 1..=1, 1..=1).is_keyword());
        assert!(tok(TokenKind::Or, "", 1..=1, 1..=1).is_keyword());
        assert!(tok(TokenKind::Ref, "", 1..=1, 1..=1).is_keyword());
        assert!(tok(TokenKind::Return, "", 1..=1, 1..=1).is_keyword());
        assert!(tok(TokenKind::SelfObject, "", 1..=1, 1..=1).is_keyword());
        assert!(tok(TokenKind::Static, "", 1..=1, 1..=1).is_keyword());
        assert!(tok(TokenKind::Throw, "", 1..=1, 1..=1).is_keyword());
        assert!(tok(TokenKind::Trait, "", 1..=1, 1..=1).is_keyword());
        assert!(tok(TokenKind::True, "", 1..=1, 1..=1).is_keyword());
        assert!(tok(TokenKind::Try, "", 1..=1, 1..=1).is_keyword());
        assert!(tok(TokenKind::While, "", 1..=1, 1..=1).is_keyword());
        assert!(tok(TokenKind::Recover, "", 1..=1, 1..=1).is_keyword());
        assert!(tok(TokenKind::Nil, "", 1..=1, 1..=1).is_keyword());
    }

    #[test]
    fn test_token_is_operator() {
        assert!(tok(TokenKind::Add, "", 1..=1, 1..=1).is_operator());
        assert!(tok(TokenKind::Sub, "", 1..=1, 1..=1).is_operator());
        assert!(tok(TokenKind::Div, "", 1..=1, 1..=1).is_operator());
        assert!(tok(TokenKind::Mul, "", 1..=1, 1..=1).is_operator());
        assert!(tok(TokenKind::Mod, "", 1..=1, 1..=1).is_operator());
        assert!(tok(TokenKind::Pow, "", 1..=1, 1..=1).is_operator());
        assert!(tok(TokenKind::BitAnd, "", 1..=1, 1..=1).is_operator());
        assert!(tok(TokenKind::BitOr, "", 1..=1, 1..=1).is_operator());
        assert!(tok(TokenKind::BitXor, "", 1..=1, 1..=1).is_operator());
        assert!(tok(TokenKind::Shl, "", 1..=1, 1..=1).is_operator());
        assert!(tok(TokenKind::Shr, "", 1..=1, 1..=1).is_operator());
        assert!(tok(TokenKind::Lt, "", 1..=1, 1..=1).is_operator());
        assert!(tok(TokenKind::Le, "", 1..=1, 1..=1).is_operator());
        assert!(tok(TokenKind::Gt, "", 1..=1, 1..=1).is_operator());
        assert!(tok(TokenKind::Ge, "", 1..=1, 1..=1).is_operator());
        assert!(tok(TokenKind::Eq, "", 1..=1, 1..=1).is_operator());
        assert!(tok(TokenKind::Ne, "", 1..=1, 1..=1).is_operator());
        assert!(tok(TokenKind::UnsignedShr, "", 1..=1, 1..=1).is_operator());
    }

    #[test]
    fn test_token_same_line_as() {
        let tok1 = tok(TokenKind::As, "", 1..=1, 1..=1);
        let tok2 = tok(TokenKind::As, "", 1..=1, 1..=1);
        let tok3 = tok(TokenKind::As, "", 2..=2, 1..=1);

        assert!(tok1.same_line_as(&tok2));
        assert!(!tok1.same_line_as(&tok3));
    }

    #[test]
    fn test_lexer_integer() {
        assert_token!("10", Integer, "10", 1..=1, 1..=2);
        assert_token!("10x", Integer, "10", 1..=1, 1..=2);
        assert_token!("10_20_30", Integer, "10_20_30", 1..=1, 1..=8);
        assert_token!("0xaf", Integer, "0xaf", 1..=1, 1..=4);
        assert_token!("0xFF", Integer, "0xFF", 1..=1, 1..=4);
        assert_token!("0xF_F", Integer, "0xF_F", 1..=1, 1..=5);
        assert_token!("10Ea", Integer, "10", 1..=1, 1..=2);
        assert_token!("10.+5", Integer, "10", 1..=1, 1..=2);
    }

    #[test]
    fn test_lexer_float() {
        assert_token!("10.5", Float, "10.5", 1..=1, 1..=4);
        assert_token!("10.5x", Float, "10.5", 1..=1, 1..=4);
        assert_token!("10e5", Float, "10e5", 1..=1, 1..=4);
        assert_token!("10E5", Float, "10E5", 1..=1, 1..=4);
        assert_token!("10.2e5", Float, "10.2e5", 1..=1, 1..=6);
        assert_token!("1_0.2e5", Float, "1_0.2e5", 1..=1, 1..=7);
        assert_token!("10e+5", Float, "10e+5", 1..=1, 1..=5);
        assert_token!("10e-5", Float, "10e-5", 1..=1, 1..=5);
        assert_token!("10E+5", Float, "10E+5", 1..=1, 1..=5);
        assert_token!("10E-5", Float, "10E-5", 1..=1, 1..=5);
        assert_token!(
            "1_000_000_000.0",
            Float,
            "1_000_000_000.0",
            1..=1,
            1..=15
        );
    }

    #[test]
    fn test_lexer_field() {
        assert_token!("@foo", Field, "foo", 1..=1, 1..=4);
        assert_token!("@foo_bar", Field, "foo_bar", 1..=1, 1..=8);
        assert_token!("@foo1", Field, "foo1", 1..=1, 1..=5);
        assert_token!("@0", Field, "0", 1..=1, 1..=2);
        assert_token!("@a?", Field, "a?", 1..=1, 1..=3);
        assert_token!("@a?b", Field, "a?", 1..=1, 1..=3);
    }

    #[test]
    fn test_lexer_comment() {
        assert_token!("#foo", Comment, "foo", 1..=1, 1..=4);
        assert_token!("# foo", Comment, "foo", 1..=1, 1..=5);
        assert_token!("# foo\nbar", Comment, "foo", 1..=1, 1..=5);
        assert_token!("# €€€", Comment, "€€€", 1..=1, 1..=5);
    }

    #[test]
    fn test_lexer_curly_braces() {
        assert_token!("{", CurlyOpen, "{", 1..=1, 1..=1);
        assert_token!("}", CurlyClose, "}", 1..=1, 1..=1);
    }

    #[test]
    fn test_lexer_curly_brace_balancing() {
        let mut lexer = lexer("{}");

        assert_eq!(lexer.next_token(), tok(CurlyOpen, "{", 1..=1, 1..=1));
        assert_eq!(lexer.next_token(), tok(CurlyClose, "}", 1..=1, 2..=2));
    }

    #[test]
    fn test_lexer_parentheses() {
        assert_token!("(", ParenOpen, "(", 1..=1, 1..=1);
        assert_token!(")", ParenClose, ")", 1..=1, 1..=1);
    }

    #[test]
    fn test_lexer_parentheses_balancing() {
        let mut lexer = lexer("()");

        assert_eq!(lexer.next_token(), tok(ParenOpen, "(", 1..=1, 1..=1));
        assert_eq!(lexer.next_token(), tok(ParenClose, ")", 1..=1, 2..=2));
    }

    #[test]
    fn text_lexer_whitespace() {
        assert_token!("\t", Whitespace, "\t", 1..=1, 1..=1);
        assert_token!(" ", Whitespace, " ", 1..=1, 1..=1);
        assert_token!("\r", Whitespace, "\r", 1..=1, 1..=1);
        assert_tokens!(
            " 10 \t\r",
            tok(Whitespace, " ", 1..=1, 1..=1),
            tok(Integer, "10", 1..=1, 2..=3),
            tok(Whitespace, " \t\r", 1..=1, 4..=6)
        );
        assert_tokens!(
            "\n10\n",
            tok(Whitespace, "\n", 1..=1, 1..=1),
            tok(Integer, "10", 2..=2, 1..=2),
            tok(Whitespace, "\n", 2..=2, 3..=3)
        );
        assert_tokens!(
            " \n ",
            tok(Whitespace, " \n", 1..=1, 1..=2),
            tok(Whitespace, " ", 2..=2, 1..=1)
        );
    }

    #[test]
    fn test_lexer_single_quoted_string() {
        assert_tokens!(
            "''",
            tok(SingleStringOpen, "'", 1..=1, 1..=1),
            tok(SingleStringClose, "'", 1..=1, 2..=2)
        );
        assert_tokens!(
            "'foo'",
            tok(SingleStringOpen, "'", 1..=1, 1..=1),
            tok(StringText, "foo", 1..=1, 2..=4),
            tok(SingleStringClose, "'", 1..=1, 5..=5)
        );
        assert_tokens!(
            "'\nfoo'",
            tok(SingleStringOpen, "'", 1..=1, 1..=1),
            tok(StringText, "\n", 1..=1, 2..=2),
            tok(StringText, "foo", 2..=2, 1..=3),
            tok(SingleStringClose, "'", 2..=2, 4..=4)
        );
        assert_tokens!(
            "'foo\nbar'",
            tok(SingleStringOpen, "'", 1..=1, 1..=1),
            tok(StringText, "foo\n", 1..=1, 2..=5),
            tok(StringText, "bar", 2..=2, 1..=3),
            tok(SingleStringClose, "'", 2..=2, 4..=4)
        );
        assert_tokens!(
            "'foo\n'",
            tok(SingleStringOpen, "'", 1..=1, 1..=1),
            tok(StringText, "foo\n", 1..=1, 2..=5),
            tok(SingleStringClose, "'", 2..=2, 1..=1)
        );
        assert_tokens!(
            "'foo\n '",
            tok(SingleStringOpen, "'", 1..=1, 1..=1),
            tok(StringText, "foo\n", 1..=1, 2..=5),
            tok(StringText, " ", 2..=2, 1..=1),
            tok(SingleStringClose, "'", 2..=2, 2..=2)
        );
        assert_tokens!(
            "'foo\\xbar'",
            tok(SingleStringOpen, "'", 1..=1, 1..=1),
            tok(StringText, "foo\\xbar", 1..=1, 2..=9),
            tok(SingleStringClose, "'", 1..=1, 10..=10)
        );
        assert_tokens!(
            "'foo\\'bar'",
            tok(SingleStringOpen, "'", 1..=1, 1..=1),
            tok(StringText, "foo\'bar", 1..=1, 2..=9),
            tok(SingleStringClose, "'", 1..=1, 10..=10)
        );
        assert_tokens!(
            "'foo\\\\bar'",
            tok(SingleStringOpen, "'", 1..=1, 1..=1),
            tok(StringText, "foo\\bar", 1..=1, 2..=9),
            tok(SingleStringClose, "'", 1..=1, 10..=10)
        );
        assert_tokens!(
            "'foo\\nbar'",
            tok(SingleStringOpen, "'", 1..=1, 1..=1),
            tok(StringText, "foo\\nbar", 1..=1, 2..=9),
            tok(SingleStringClose, "'", 1..=1, 10..=10)
        );
        assert_tokens!(
            "'\u{65}\u{301}'",
            tok(SingleStringOpen, "'", 1..=1, 1..=1),
            tok(StringText, "\u{65}\u{301}", 1..=1, 2..=2),
            tok(SingleStringClose, "'", 1..=1, 3..=3)
        );
        assert_tokens!(
            "'🇳🇱'",
            tok(SingleStringOpen, "'", 1..=1, 1..=1),
            tok(StringText, "🇳🇱", 1..=1, 2..=2),
            tok(SingleStringClose, "'", 1..=1, 3..=3)
        );
        assert_tokens!("'", tok(SingleStringOpen, "'", 1..=1, 1..=1));
        assert_tokens!(
            "'foo",
            tok(SingleStringOpen, "'", 1..=1, 1..=1),
            tok(StringText, "foo", 1..=1, 2..=4)
        );
    }

    #[test]
    fn test_lexer_double_quoted_string() {
        assert_tokens!(
            "\"\"",
            tok(DoubleStringOpen, "\"", 1..=1, 1..=1),
            tok(DoubleStringClose, "\"", 1..=1, 2..=2)
        );
        assert_tokens!(
            "\"foo\"",
            tok(DoubleStringOpen, "\"", 1..=1, 1..=1),
            tok(StringText, "foo", 1..=1, 2..=4),
            tok(DoubleStringClose, "\"", 1..=1, 5..=5)
        );
        assert_tokens!(
            "\"\nfoo\"",
            tok(DoubleStringOpen, "\"", 1..=1, 1..=1),
            tok(StringText, "\n", 1..=1, 2..=2),
            tok(StringText, "foo", 2..=2, 1..=3),
            tok(DoubleStringClose, "\"", 2..=2, 4..=4)
        );
        assert_tokens!(
            "\"foo\nbar\"",
            tok(DoubleStringOpen, "\"", 1..=1, 1..=1),
            tok(StringText, "foo\n", 1..=1, 2..=5),
            tok(StringText, "bar", 2..=2, 1..=3),
            tok(DoubleStringClose, "\"", 2..=2, 4..=4)
        );
        assert_tokens!(
            "\"foo\n\"",
            tok(DoubleStringOpen, "\"", 1..=1, 1..=1),
            tok(StringText, "foo\n", 1..=1, 2..=5),
            tok(DoubleStringClose, "\"", 2..=2, 1..=1)
        );
        assert_tokens!(
            "\"foo\n \"",
            tok(DoubleStringOpen, "\"", 1..=1, 1..=1),
            tok(StringText, "foo\n", 1..=1, 2..=5),
            tok(StringText, " ", 2..=2, 1..=1),
            tok(DoubleStringClose, "\"", 2..=2, 2..=2)
        );
        assert_tokens!(
            "\"foo\\xbar\"",
            tok(DoubleStringOpen, "\"", 1..=1, 1..=1),
            tok(StringText, "foo\\xbar", 1..=1, 2..=9),
            tok(DoubleStringClose, "\"", 1..=1, 10..=10)
        );
        assert_tokens!(
            "\"foo\\\"bar\"",
            tok(DoubleStringOpen, "\"", 1..=1, 1..=1),
            tok(StringText, "foo\"bar", 1..=1, 2..=9),
            tok(DoubleStringClose, "\"", 1..=1, 10..=10)
        );
        assert_tokens!(
            "\"foo\\\\bar\"",
            tok(DoubleStringOpen, "\"", 1..=1, 1..=1),
            tok(StringText, "foo\\bar", 1..=1, 2..=9),
            tok(DoubleStringClose, "\"", 1..=1, 10..=10)
        );
        assert_tokens!(
            "\"foo\\nbar\"",
            tok(DoubleStringOpen, "\"", 1..=1, 1..=1),
            tok(StringText, "foo\nbar", 1..=1, 2..=9),
            tok(DoubleStringClose, "\"", 1..=1, 10..=10)
        );
        assert_tokens!(
            "\"foo\\tbar\"",
            tok(DoubleStringOpen, "\"", 1..=1, 1..=1),
            tok(StringText, "foo\tbar", 1..=1, 2..=9),
            tok(DoubleStringClose, "\"", 1..=1, 10..=10)
        );
        assert_tokens!(
            "\"foo\\rbar\"",
            tok(DoubleStringOpen, "\"", 1..=1, 1..=1),
            tok(StringText, "foo\rbar", 1..=1, 2..=9),
            tok(DoubleStringClose, "\"", 1..=1, 10..=10)
        );
        assert_tokens!(
            "\"\u{65}\u{301}\"",
            tok(DoubleStringOpen, "\"", 1..=1, 1..=1),
            tok(StringText, "\u{65}\u{301}", 1..=1, 2..=2),
            tok(DoubleStringClose, "\"", 1..=1, 3..=3)
        );
        assert_tokens!(
            "\"🇳🇱\"",
            tok(DoubleStringOpen, "\"", 1..=1, 1..=1),
            tok(StringText, "🇳🇱", 1..=1, 2..=2),
            tok(DoubleStringClose, "\"", 1..=1, 3..=3)
        );
        assert_tokens!("\"", tok(DoubleStringOpen, "\"", 1..=1, 1..=1));
        assert_tokens!(
            "\"foo",
            tok(DoubleStringOpen, "\"", 1..=1, 1..=1),
            tok(StringText, "foo", 1..=1, 2..=4)
        );
        assert_tokens!(
            "\"\\{}\"",
            tok(DoubleStringOpen, "\"", 1..=1, 1..=1),
            tok(StringText, "{}", 1..=1, 2..=4),
            tok(DoubleStringClose, "\"", 1..=1, 5..=5)
        );
    }

    #[test]
    fn test_lexer_double_string_unicode_escapes() {
        assert_tokens!(
            "\"\\u{AC}\"",
            tok(DoubleStringOpen, "\"", 1..=1, 1..=1),
            tok(UnicodeEscape, "\u{AC}", 1..=1, 2..=7),
            tok(DoubleStringClose, "\"", 1..=1, 8..=8)
        );
        assert_tokens!(
            "\"a\\u{AC}b\"",
            tok(DoubleStringOpen, "\"", 1..=1, 1..=1),
            tok(StringText, "a", 1..=1, 2..=2),
            tok(UnicodeEscape, "\u{AC}", 1..=1, 3..=8),
            tok(StringText, "b", 1..=1, 9..=9),
            tok(DoubleStringClose, "\"", 1..=1, 10..=10)
        );
        assert_tokens!(
            "\"a\\u{FFFFF}b\"",
            tok(DoubleStringOpen, "\"", 1..=1, 1..=1),
            tok(StringText, "a", 1..=1, 2..=2),
            tok(UnicodeEscape, "\u{FFFFF}", 1..=1, 3..=11),
            tok(StringText, "b", 1..=1, 12..=12),
            tok(DoubleStringClose, "\"", 1..=1, 13..=13)
        );
        assert_tokens!(
            "\"a\\u{10FFFF}b\"",
            tok(DoubleStringOpen, "\"", 1..=1, 1..=1),
            tok(StringText, "a", 1..=1, 2..=2),
            tok(UnicodeEscape, "\u{10FFFF}", 1..=1, 3..=12),
            tok(StringText, "b", 1..=1, 13..=13),
            tok(DoubleStringClose, "\"", 1..=1, 14..=14)
        );
        assert_tokens!(
            "\"a\\u{XXXXX}b\"",
            tok(DoubleStringOpen, "\"", 1..=1, 1..=1),
            tok(StringText, "a", 1..=1, 2..=2),
            tok(InvalidUnicodeEscape, "XXXXX", 1..=1, 3..=11),
            tok(StringText, "b", 1..=1, 12..=12),
            tok(DoubleStringClose, "\"", 1..=1, 13..=13)
        );
        assert_tokens!(
            "\"a\\u{FFFFFF}b\"",
            tok(DoubleStringOpen, "\"", 1..=1, 1..=1),
            tok(StringText, "a", 1..=1, 2..=2),
            tok(InvalidUnicodeEscape, "FFFFFF", 1..=1, 3..=12),
            tok(StringText, "b", 1..=1, 13..=13),
            tok(DoubleStringClose, "\"", 1..=1, 14..=14)
        );
        assert_tokens!(
            "\"a\\u{AAAAA #}b\"",
            tok(DoubleStringOpen, "\"", 1..=1, 1..=1),
            tok(StringText, "a", 1..=1, 2..=2),
            tok(InvalidUnicodeEscape, "AAAAA #", 1..=1, 3..=13),
            tok(StringText, "b", 1..=1, 14..=14),
            tok(DoubleStringClose, "\"", 1..=1, 15..=15)
        );
        assert_tokens!(
            "\"a\\u{€}b\"",
            tok(DoubleStringOpen, "\"", 1..=1, 1..=1),
            tok(StringText, "a", 1..=1, 2..=2),
            tok(InvalidUnicodeEscape, "€", 1..=1, 3..=7),
            tok(StringText, "b", 1..=1, 8..=8),
            tok(DoubleStringClose, "\"", 1..=1, 9..=9)
        );
        assert_tokens!(
            "\"a\\u{🇳🇱}b\"",
            tok(DoubleStringOpen, "\"", 1..=1, 1..=1),
            tok(StringText, "a", 1..=1, 2..=2),
            tok(InvalidUnicodeEscape, "🇳🇱", 1..=1, 3..=7),
            tok(StringText, "b", 1..=1, 8..=8),
            tok(DoubleStringClose, "\"", 1..=1, 9..=9)
        );
        assert_tokens!(
            "\"\\u{AA\"",
            tok(DoubleStringOpen, "\"", 1..=1, 1..=1),
            tok(InvalidUnicodeEscape, "AA", 1..=1, 2..=6),
            tok(DoubleStringClose, "\"", 1..=1, 7..=7)
        );
    }

    #[test]
    fn test_lexer_double_string_with_expressions() {
        assert_tokens!(
            "\"foo{10}baz\"",
            tok(DoubleStringOpen, "\"", 1..=1, 1..=1),
            tok(StringText, "foo", 1..=1, 2..=4),
            tok(StringExprOpen, "{", 1..=1, 5..=5),
            tok(Integer, "10", 1..=1, 6..=7),
            tok(StringExprClose, "}", 1..=1, 8..=8),
            tok(StringText, "baz", 1..=1, 9..=11),
            tok(DoubleStringClose, "\"", 1..=1, 12..=12)
        );
        assert_tokens!(
            "\"{\"{10}\"}\"",
            tok(DoubleStringOpen, "\"", 1..=1, 1..=1),
            tok(StringExprOpen, "{", 1..=1, 2..=2),
            tok(DoubleStringOpen, "\"", 1..=1, 3..=3),
            tok(StringExprOpen, "{", 1..=1, 4..=4),
            tok(Integer, "10", 1..=1, 5..=6),
            tok(StringExprClose, "}", 1..=1, 7..=7),
            tok(DoubleStringClose, "\"", 1..=1, 8..=8),
            tok(StringExprClose, "}", 1..=1, 9..=9),
            tok(DoubleStringClose, "\"", 1..=1, 10..=10)
        );
    }

    #[test]
    fn test_lexer_double_string_with_unclosed_expression() {
        assert_tokens!(
            "\"foo{10 +\"",
            tok(DoubleStringOpen, "\"", 1..=1, 1..=1),
            tok(StringText, "foo", 1..=1, 2..=4),
            tok(StringExprOpen, "{", 1..=1, 5..=5),
            tok(Integer, "10", 1..=1, 6..=7),
            tok(Whitespace, " ", 1..=1, 8..=8),
            tok(Add, "+", 1..=1, 9..=9),
            tok(DoubleStringOpen, "\"", 1..=1, 10..=10)
        );
    }

    #[test]
    fn test_lexer_single_quoted_string_with_escaped_whitespace() {
        assert_tokens!(
            "'foo \\\n  bar'",
            tok(SingleStringOpen, "'", 1..=1, 1..=1),
            tok(StringText, "foo ", 1..=1, 2..=6),
            tok(StringText, "bar", 2..=2, 3..=5),
            tok(SingleStringClose, "'", 2..=2, 6..=6)
        );
    }

    #[test]
    fn test_lexer_double_quoted_string_with_escaped_whitespace() {
        assert_tokens!(
            "\"foo \\\n  bar\"",
            tok(DoubleStringOpen, "\"", 1..=1, 1..=1),
            tok(StringText, "foo ", 1..=1, 2..=6),
            tok(StringText, "bar", 2..=2, 3..=5),
            tok(DoubleStringClose, "\"", 2..=2, 6..=6)
        );
    }

    #[test]
    fn test_lexer_colon() {
        assert_token!(":", Colon, ":", 1..=1, 1..=1);
        assert_token!(":=", Replace, ":=", 1..=1, 1..=2);
    }

    #[test]
    fn test_lexer_percent() {
        assert_token!("%", Mod, "%", 1..=1, 1..=1);
        assert_token!("%=", ModAssign, "%=", 1..=1, 1..=2);
    }

    #[test]
    fn test_lexer_slash() {
        assert_token!("/", Div, "/", 1..=1, 1..=1);
        assert_token!("/=", DivAssign, "/=", 1..=1, 1..=2);
    }

    #[test]
    fn test_lexer_bitwise_xor() {
        assert_token!("^", BitXor, "^", 1..=1, 1..=1);
        assert_token!("^=", BitXorAssign, "^=", 1..=1, 1..=2);
    }

    #[test]
    fn test_lexer_bitwise_and() {
        assert_token!("&", BitAnd, "&", 1..=1, 1..=1);
        assert_token!("&=", BitAndAssign, "&=", 1..=1, 1..=2);
    }

    #[test]
    fn test_lexer_bitwise_or() {
        assert_token!("|", BitOr, "|", 1..=1, 1..=1);
        assert_token!("|=", BitOrAssign, "|=", 1..=1, 1..=2);
    }

    #[test]
    fn test_lexer_star() {
        assert_token!("*", Mul, "*", 1..=1, 1..=1);
        assert_token!("*=", MulAssign, "*=", 1..=1, 1..=2);
        assert_token!("**", Pow, "**", 1..=1, 1..=2);
        assert_token!("**=", PowAssign, "**=", 1..=1, 1..=3);
    }

    #[test]
    fn test_lexer_minus() {
        assert_token!("-", Sub, "-", 1..=1, 1..=1);
        assert_token!("-=", SubAssign, "-=", 1..=1, 1..=2);
        assert_token!("->", Arrow, "->", 1..=1, 1..=2);
        assert_tokens!("-10", tok(Integer, "-10", 1..=1, 1..=3));
        assert_tokens!("-10.5", tok(Float, "-10.5", 1..=1, 1..=5));
        assert_tokens!(
            "10 - 20",
            tok(Integer, "10", 1..=1, 1..=2),
            tok(Whitespace, " ", 1..=1, 3..=3),
            tok(Sub, "-", 1..=1, 4..=4),
            tok(Whitespace, " ", 1..=1, 5..=5),
            tok(Integer, "20", 1..=1, 6..=7)
        );
    }

    #[test]
    fn test_lexer_plus() {
        assert_token!("+", Add, "+", 1..=1, 1..=1);
        assert_token!("+=", AddAssign, "+=", 1..=1, 1..=2);
    }

    #[test]
    fn test_lexer_equal() {
        assert_token!("=", Assign, "=", 1..=1, 1..=1);
        assert_token!("==", Eq, "==", 1..=1, 1..=2);
        assert_token!("=>", DoubleArrow, "=>", 1..=1, 1..=2);
    }

    #[test]
    fn test_lexer_less() {
        assert_token!("<", Lt, "<", 1..=1, 1..=1);
        assert_token!("<=", Le, "<=", 1..=1, 1..=2);
        assert_token!("<<", Shl, "<<", 1..=1, 1..=2);
        assert_token!("<<=", ShlAssign, "<<=", 1..=1, 1..=3);
    }

    #[test]
    fn test_lexer_greater() {
        assert_token!(">", Gt, ">", 1..=1, 1..=1);
        assert_token!(">=", Ge, ">=", 1..=1, 1..=2);
        assert_token!(">>", Shr, ">>", 1..=1, 1..=2);
        assert_token!(">>=", ShrAssign, ">>=", 1..=1, 1..=3);
        assert_token!(">>>", UnsignedShr, ">>>", 1..=1, 1..=3);
        assert_token!(">>>=", UnsignedShrAssign, ">>>=", 1..=1, 1..=4);
    }

    #[test]
    fn test_lexer_brackets() {
        assert_token!("[", BracketOpen, "[", 1..=1, 1..=1);
        assert_token!("]", BracketClose, "]", 1..=1, 1..=1);
    }

    #[test]
    fn test_lexer_exclamation() {
        assert_token!("!", Invalid, "!", 1..=1, 1..=1);
        assert_token!("!=", Ne, "!=", 1..=1, 1..=2);
    }

    #[test]
    fn test_lexer_dot() {
        assert_token!(".", Dot, ".", 1..=1, 1..=1);
    }

    #[test]
    fn test_lexer_comma() {
        assert_token!(",", Comma, ",", 1..=1, 1..=1);
    }

    #[test]
    fn test_lexer_keywords() {
        assert_token!("as", As, "as", 1..=1, 1..=2);
        assert_token!("fn", Fn, "fn", 1..=1, 1..=2);
        assert_token!("if", If, "if", 1..=1, 1..=2);
        assert_token!("or", Or, "or", 1..=1, 1..=2);

        assert_token!("and", And, "and", 1..=1, 1..=3);
        assert_token!("for", For, "for", 1..=1, 1..=3);
        assert_token!("let", Let, "let", 1..=1, 1..=3);
        assert_token!("ref", Ref, "ref", 1..=1, 1..=3);
        assert_token!("try", Try, "try", 1..=1, 1..=3);
        assert_token!("mut", Mut, "mut", 1..=1, 1..=3);
        assert_token!("uni", Uni, "uni", 1..=1, 1..=3);
        assert_token!("pub", Pub, "pub", 1..=1, 1..=3);
        assert_token!("nil", Nil, "nil", 1..=1, 1..=3);

        assert_token!("else", Else, "else", 1..=1, 1..=4);
        assert_token!("impl", Implement, "impl", 1..=1, 1..=4);
        assert_token!("loop", Loop, "loop", 1..=1, 1..=4);
        assert_token!("next", Next, "next", 1..=1, 1..=4);
        assert_token!("self", SelfObject, "self", 1..=1, 1..=4);
        assert_token!("move", Move, "move", 1..=1, 1..=4);
        assert_token!("true", True, "true", 1..=1, 1..=4);
        assert_token!("case", Case, "case", 1..=1, 1..=4);
        assert_token!("enum", Enum, "enum", 1..=1, 1..=4);

        assert_token!("class", Class, "class", 1..=1, 1..=5);
        assert_token!("async", Async, "async", 1..=1, 1..=5);
        assert_token!("break", Break, "break", 1..=1, 1..=5);
        assert_token!("match", Match, "match", 1..=1, 1..=5);
        assert_token!("throw", Throw, "throw", 1..=1, 1..=5);
        assert_token!("trait", Trait, "trait", 1..=1, 1..=5);
        assert_token!("while", While, "while", 1..=1, 1..=5);
        assert_token!("false", False, "false", 1..=1, 1..=5);

        assert_token!("import", Import, "import", 1..=1, 1..=6);
        assert_token!("return", Return, "return", 1..=1, 1..=6);
        assert_token!("static", Static, "static", 1..=1, 1..=6);
        assert_token!("extern", Extern, "extern", 1..=1, 1..=6);

        assert_token!("builtin", Builtin, "builtin", 1..=1, 1..=7);
        assert_token!("recover", Recover, "recover", 1..=1, 1..=7);
    }

    #[test]
    fn test_lexer_identifiers() {
        assert_token!("foo", Identifier, "foo", 1..=1, 1..=3);
        assert_token!("foo$bar", Identifier, "foo$bar", 1..=1, 1..=7);
        assert_token!("baz", Identifier, "baz", 1..=1, 1..=3);
        assert_token!("foo_bar", Identifier, "foo_bar", 1..=1, 1..=7);
        assert_token!("foo_BAR", Identifier, "foo_BAR", 1..=1, 1..=7);
        assert_token!("foo_123", Identifier, "foo_123", 1..=1, 1..=7);
        assert_token!("foo_123a", Identifier, "foo_123a", 1..=1, 1..=8);
        assert_token!("foo_123a_", Identifier, "foo_123a_", 1..=1, 1..=9);
        assert_token!("_foo_123a", Identifier, "_foo_123a", 1..=1, 1..=9);
        assert_token!("__foo_123a", Identifier, "__foo_123a", 1..=1, 1..=10);
        assert_token!("_", Identifier, "_", 1..=1, 1..=1);
        assert_token!("__", Identifier, "__", 1..=1, 1..=2);
        assert_token!("_0", Identifier, "_0", 1..=1, 1..=2);
        assert_token!("_9", Identifier, "_9", 1..=1, 1..=2);
        assert_token!("__1", Identifier, "__1", 1..=1, 1..=3);
        assert_token!("foo?", Identifier, "foo?", 1..=1, 1..=4);
    }

    #[test]
    fn test_lexer_constants() {
        assert_token!("FOO", Constant, "FOO", 1..=1, 1..=3);
        assert_token!("FOO?", Constant, "FOO?", 1..=1, 1..=4);
        assert_token!("FOO_bar", Constant, "FOO_bar", 1..=1, 1..=7);
        assert_token!("FOO_BAR", Constant, "FOO_BAR", 1..=1, 1..=7);
        assert_token!("FOO_123", Constant, "FOO_123", 1..=1, 1..=7);
        assert_token!("FOO_123a", Constant, "FOO_123a", 1..=1, 1..=8);
        assert_token!("FOO_123a_", Constant, "FOO_123a_", 1..=1, 1..=9);
        assert_token!("_FOO_123a", Constant, "_FOO_123a", 1..=1, 1..=9);
        assert_token!("__FOO_123a", Constant, "__FOO_123a", 1..=1, 1..=10);
    }

    #[test]
    fn test_lexer_null_empty() {
        let mut lexer = lexer("");

        assert_eq!(lexer.next_token(), tok(Null, "", 1..=1, 1..=1));
    }

    #[test]
    fn test_lexer_null_token() {
        let mut lexer = lexer("  ");

        assert_eq!(lexer.next_token(), tok(Whitespace, "  ", 1..=1, 1..=2));
        assert_eq!(lexer.next_token(), tok(Null, "", 1..=1, 2..=2));
    }

    #[test]
    fn test_lexer_null_after_newline() {
        let mut lexer = lexer("\n");

        assert_eq!(lexer.next_token(), tok(Whitespace, "\n", 1..=1, 1..=1));
        assert_eq!(lexer.next_token(), tok(Null, "", 2..=2, 1..=1));
    }

    #[test]
    fn test_lexer_identifier_with_question_mark() {
        let mut lexer = lexer("a?b");

        assert_eq!(lexer.next_token(), tok(Identifier, "a?", 1..=1, 1..=2));
        assert_eq!(lexer.next_token(), tok(Identifier, "b", 1..=1, 3..=3));
    }
}
