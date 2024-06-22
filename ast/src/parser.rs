//! Parsing of Inko source code into an AST.
//!
//! While the parser tries to retain as much information about source code as
//! possible, it's not a lossless parser. This means that while you can
//! reconstruct Inko source code from the AST, it may not be exactly the same as
//! the input.
use crate::lexer::{Lexer, Token, TokenKind};
use crate::nodes::*;
use crate::source_location::SourceLocation;
use std::path::PathBuf;

/// Produces a parser error and returns from the surrounding function.
macro_rules! error {
    ($location: expr, $message: expr, $($field: expr),*) => {
        return Err(ParseError {
            message: format!($message, $($field),*),
            location: $location
        })
    };

    ($location: expr, $message: expr) => {
        return Err(ParseError {
            message: $message.to_string(),
            location: $location
        })
    }
}

/// Returns the source location of an optional AST node.
///
/// This macro exists so we can more easily obtain locations from optional
/// AST nodes, without having to repeat the same `x.as_ref().map(...)` pattern
/// every time.
///
/// We use a macro so we can accept `Option<T>`, `Option<&T>`, `&Option<T>`, and
/// `&Option<&T>` easily.
macro_rules! location {
    ($node: expr) => {
        $node.as_ref().map(|x| x.location())
    };
}

/// An error produced when encountering invalid syntax.
#[derive(Debug)]
pub struct ParseError {
    /// A message describing the error.
    pub message: String,

    /// The location the error originated from.
    pub location: SourceLocation,
}

/// A recursive-descent parser that turns Inko source code into an AST.
///
/// The AST is not a lossless AST. For example, whitespace and comments are not
/// preserved. Reconstructing source code from an AST should be possible, but
/// you wouldn't be able to reproduce the exact same source code.
pub struct Parser {
    file: PathBuf,
    lexer: Lexer,
    peeked: Option<Token>,
    comments: bool,
}

impl Parser {
    pub fn new(input: Vec<u8>, file: PathBuf) -> Self {
        let lexer = Lexer::new(input);

        Self { file, lexer, comments: false, peeked: None }
    }

    pub fn with_comments(input: Vec<u8>, file: PathBuf) -> Self {
        let mut parser = Parser::new(input, file);

        parser.comments = true;
        parser
    }

    pub fn parse(&mut self) -> Result<Module, ParseError> {
        let start_loc = self.lexer.start_location();
        let mut expressions = Vec::new();

        loop {
            let token = self.next();

            if token.kind == TokenKind::Null {
                let file = self.file.clone();
                let location =
                    SourceLocation::start_end(&start_loc, &token.location);

                return Ok(Module { expressions, file, location });
            }

            expressions.push(self.top_level_expression(token)?);
        }
    }

    fn top_level_expression(
        &mut self,
        start: Token,
    ) -> Result<TopLevelExpression, ParseError> {
        let expr = match start.kind {
            TokenKind::Import => self.import(start)?,
            TokenKind::Class => self.define_class(start)?,
            TokenKind::Implement => self.implementation(start)?,
            TokenKind::Trait => self.define_trait(start)?,
            TokenKind::Fn => self.define_module_method(start)?,
            TokenKind::Let => self.define_constant(start)?,
            TokenKind::Comment => {
                TopLevelExpression::Comment(self.comment(start))
            }
            _ => {
                error!(
                    start.location,
                    "expected a top-level expression, found '{}' instead",
                    start.value
                );
            }
        };

        Ok(expr)
    }

    fn import(
        &mut self,
        start: Token,
    ) -> Result<TopLevelExpression, ParseError> {
        if self.peek().kind == TokenKind::Extern {
            return self.extern_import(start);
        }

        let path = self.import_path()?;
        let symbols = self.import_symbols()?;
        let tags = self.build_tags()?;
        let location = SourceLocation::start_end(
            &start.location,
            location!(tags)
                .or_else(|| location!(symbols))
                .unwrap_or(&path.location),
        );

        Ok(TopLevelExpression::Import(Box::new(Import {
            path,
            symbols,
            tags,
            location,
            include: true,
        })))
    }

    fn import_path(&mut self) -> Result<ImportPath, ParseError> {
        let mut steps = Vec::new();

        loop {
            let token = self.require()?;

            if !token.is_keyword() && token.kind != TokenKind::Identifier {
                error!(token.location, "expected an identifier or keyword");
            }

            steps.push(Identifier::from(token));

            if self.peek().kind != TokenKind::Dot {
                break;
            }

            self.next();
        }

        let start_loc = steps.first().map(|s| &s.location).unwrap();
        let end_loc = steps.last().map(|s| &s.location).unwrap();
        let location = SourceLocation::start_end(start_loc, end_loc);

        Ok(ImportPath { steps, location })
    }

    fn import_symbols(&mut self) -> Result<Option<ImportSymbols>, ParseError> {
        if self.peek().kind != TokenKind::ParenOpen {
            return Ok(None);
        }

        let (values, location) = self.list(
            TokenKind::ParenOpen,
            TokenKind::ParenClose,
            |parser, token| {
                let alias = match token.kind {
                    TokenKind::Identifier => parser.import_alias(token.kind)?,
                    TokenKind::Constant => parser.import_alias(token.kind)?,
                    TokenKind::SelfObject => {
                        parser.import_alias(TokenKind::Identifier)?
                    }
                    _ => {
                        error!(
                            token.location,
                            "expected an identifier, constant, \
                             or 'self'; found '{}' instead",
                            token.value
                        );
                    }
                };

                Ok(ImportSymbol {
                    name: token.value,
                    alias,
                    location: token.location,
                })
            },
        )?;

        Ok(Some(ImportSymbols { values, location }))
    }

    fn import_alias(
        &mut self,
        expected: TokenKind,
    ) -> Result<Option<ImportAlias>, ParseError> {
        if self.peek().kind != TokenKind::As {
            return Ok(None);
        }

        self.next();

        let token = self.require()?;

        if token.kind != expected {
            error!(
                token.location,
                "expected {}, found '{}' instead",
                expected.description(),
                token.value
            );
        }

        Ok(Some(ImportAlias { name: token.value, location: token.location }))
    }

    fn build_tags(&mut self) -> Result<Option<BuildTags>, ParseError> {
        let start = if self.peek().kind == TokenKind::If {
            self.next()
        } else {
            return Ok(None);
        };

        let mut values = Vec::new();

        loop {
            let token = self.expect(TokenKind::Identifier)?;
            let tag =
                Identifier { name: token.value, location: token.location };

            values.push(tag);

            if self.peek().kind == TokenKind::And {
                self.next();
            } else {
                break;
            }
        }

        let location = SourceLocation::start_end(
            &start.location,
            location!(values.last()).unwrap_or_else(|| &start.location),
        );

        Ok(Some(BuildTags { values, location }))
    }

    fn extern_import(
        &mut self,
        start: Token,
    ) -> Result<TopLevelExpression, ParseError> {
        // Skip the "extern".
        self.next();

        let path_start = self.require()?;
        let path = self.extern_import_path(path_start)?;
        let location =
            SourceLocation::start_end(&start.location, &path.location);

        Ok(TopLevelExpression::ExternImport(Box::new(ExternImport {
            path,
            location,
        })))
    }

    fn extern_import_path(
        &mut self,
        start: Token,
    ) -> Result<ExternImportPath, ParseError> {
        let close = match start.kind {
            TokenKind::SingleStringOpen => TokenKind::SingleStringClose,
            TokenKind::DoubleStringOpen => TokenKind::DoubleStringClose,
            _ => {
                error!(
                    start.location,
                    "expected a single or double quote, found '{}' instead",
                    start.kind.description()
                );
            }
        };

        let text = self.expect(TokenKind::StringText)?;
        let close = self.expect(close)?;
        let location =
            SourceLocation::start_end(&start.location, &close.location);

        Ok(ExternImportPath { path: text.value, location })
    }

    fn define_constant(
        &mut self,
        start: Token,
    ) -> Result<TopLevelExpression, ParseError> {
        let public = self.next_is_public();
        let name = Constant::from(self.expect(TokenKind::Constant)?);

        self.expect(TokenKind::Assign)?;

        let value_start = self.require()?;
        let value = self.const_expression(value_start)?;
        let location =
            SourceLocation::start_end(&start.location, value.location());

        Ok(TopLevelExpression::DefineConstant(Box::new(DefineConstant {
            public,
            name,
            value,
            location,
        })))
    }

    fn const_expression(
        &mut self,
        start: Token,
    ) -> Result<Expression, ParseError> {
        let mut left = self.const_value(start)?;

        while let Some(operator) = self.binary_operator() {
            let rhs_token = self.require()?;
            let right = self.const_value(rhs_token)?;
            let location =
                SourceLocation::start_end(left.location(), right.location());

            left = Expression::Binary(Box::new(Binary {
                operator,
                left,
                right,
                location,
            }));
        }

        Ok(left)
    }

    fn const_value(&mut self, start: Token) -> Result<Expression, ParseError> {
        let value = match start.kind {
            TokenKind::Float => self.float_literal(start),
            TokenKind::Integer => self.int_literal(start),
            TokenKind::True => self.true_literal(start),
            TokenKind::False => self.false_literal(start),
            TokenKind::SingleStringOpen => {
                self.string_value(start, TokenKind::SingleStringClose, false)?
            }
            TokenKind::DoubleStringOpen => {
                self.string_value(start, TokenKind::DoubleStringClose, false)?
            }
            TokenKind::Constant => self.constant_ref(start),
            TokenKind::ParenOpen => self.const_group(start)?,
            TokenKind::BracketOpen => self.const_array(start)?,
            TokenKind::Comment => Expression::Comment(self.comment(start)),
            TokenKind::Identifier => {
                self.expect(TokenKind::Dot)?;

                let source = Identifier::from(start);
                let const_tok = self.expect(TokenKind::Constant)?;
                let location = SourceLocation::start_end(
                    &source.location,
                    &const_tok.location,
                );

                Expression::Constant(Box::new(Constant {
                    source: Some(source),
                    name: const_tok.value,
                    location,
                }))
            }
            _ => {
                error!(
                    start.location,
                    "'{}' is not a valid constant value", start.value
                )
            }
        };

        Ok(value)
    }

    fn constant_ref(&mut self, start: Token) -> Expression {
        Expression::Constant(Box::new(Constant::from(start)))
    }

    fn const_group(&mut self, start: Token) -> Result<Expression, ParseError> {
        let value_token = self.require()?;
        let value = self.const_expression(value_token)?;
        let end = self.expect(TokenKind::ParenClose)?;
        let location =
            SourceLocation::start_end(&start.location, &end.location);

        Ok(Expression::Group(Box::new(Group { value, location })))
    }

    fn const_array(&mut self, start: Token) -> Result<Expression, ParseError> {
        let mut values = Vec::new();

        loop {
            let token = self.require()?;

            if token.kind == TokenKind::BracketClose {
                let location =
                    SourceLocation::start_end(&start.location, &token.location);

                return Ok(Expression::Array(Box::new(Array {
                    values,
                    location,
                })));
            }

            values.push(self.const_expression(token)?);

            if self.peek().kind == TokenKind::Comma {
                self.next();
            }
        }
    }

    fn optional_type_annotation(&mut self) -> Result<Option<Type>, ParseError> {
        if self.peek().kind == TokenKind::Colon {
            self.next();

            let start = self.require()?;

            Ok(Some(self.type_reference(start)?))
        } else {
            Ok(None)
        }
    }

    fn type_reference(&mut self, start: Token) -> Result<Type, ParseError> {
        let node = match start.kind {
            TokenKind::Constant => {
                Type::Named(Box::new(self.type_name(start)?))
            }
            TokenKind::Identifier => {
                Type::Named(Box::new(self.namespaced_type_name(start)?))
            }
            TokenKind::Fn => Type::Closure(Box::new(self.closure_type(start)?)),
            TokenKind::Ref => Type::Ref(Box::new(self.reference_type(start)?)),
            TokenKind::Mut => Type::Mut(Box::new(self.reference_type(start)?)),
            TokenKind::Move => {
                Type::Owned(Box::new(self.reference_type(start)?))
            }
            TokenKind::Uni => Type::Uni(Box::new(self.reference_type(start)?)),
            TokenKind::ParenOpen => {
                Type::Tuple(Box::new(self.tuple_type(start)?))
            }
            _ => error!(
                start.location,
                "expected a type name, 'fn', 'ref', 'mut', 'uni' \
                or a tuple; found a '{}' instead",
                start.value
            ),
        };

        Ok(node)
    }

    fn type_name(&mut self, start: Token) -> Result<TypeName, ParseError> {
        let name = Constant::from(start);
        let arguments = self.optional_type_parameters()?;
        let end_loc = arguments
            .as_ref()
            .map(|a| &a.location)
            .unwrap_or_else(|| name.location());
        let location = SourceLocation::start_end(name.location(), end_loc);

        Ok(TypeName { name, arguments, location })
    }

    fn namespaced_type_name(
        &mut self,
        start: Token,
    ) -> Result<TypeName, ParseError> {
        self.expect(TokenKind::Dot)?;

        let source = Identifier::from(start);
        let name_token = self.expect(TokenKind::Constant)?;
        let arguments = self.optional_type_parameters()?;
        let end_loc = arguments
            .as_ref()
            .map(|a| &a.location)
            .unwrap_or(&name_token.location);
        let location = SourceLocation::start_end(source.location(), end_loc);
        let name = Constant {
            source: Some(source),
            name: name_token.value,
            location: name_token.location,
        };

        Ok(TypeName { name, arguments, location })
    }

    fn type_name_with_optional_namespace(
        &mut self,
        start: Token,
    ) -> Result<TypeName, ParseError> {
        match start.kind {
            TokenKind::Constant => self.type_name(start),
            TokenKind::Identifier => self.namespaced_type_name(start),
            _ => {
                error!(
                    start.location,
                    "expected a constant or identifier, found '{}'",
                    start.value
                );
            }
        }
    }

    fn optional_type_parameters(
        &mut self,
    ) -> Result<Option<Types>, ParseError> {
        if self.peek().kind != TokenKind::BracketOpen {
            return Ok(None);
        }

        let (values, location) = self.list(
            TokenKind::BracketOpen,
            TokenKind::BracketClose,
            |parser, token| parser.type_reference(token),
        )?;

        Ok(Some(Types { values, location }))
    }

    fn optional_type_parameter_definitions(
        &mut self,
    ) -> Result<Option<TypeParameters>, ParseError> {
        if self.peek().kind != TokenKind::BracketOpen {
            return Ok(None);
        }

        let (values, location) = self.list(
            TokenKind::BracketOpen,
            TokenKind::BracketClose,
            |parser, token| parser.define_type_parameter(token),
        )?;

        Ok(Some(TypeParameters { values, location }))
    }

    fn define_type_parameter(
        &mut self,
        start: Token,
    ) -> Result<TypeParameter, ParseError> {
        self.require_token_kind(&start, TokenKind::Constant)?;

        let name = Constant::from(start);
        let requirements = self.optional_type_parameter_requirements()?;
        let end_loc =
            location!(requirements).unwrap_or_else(|| name.location());
        let location = SourceLocation::start_end(name.location(), end_loc);

        Ok(TypeParameter { name, requirements, location })
    }

    fn optional_trait_requirements(
        &mut self,
    ) -> Result<Option<TypeNames>, ParseError> {
        if self.peek().kind != TokenKind::Colon {
            return Ok(None);
        }

        self.next();

        let mut values = Vec::new();

        loop {
            let token = self.require()?;

            values.push(self.type_name_with_optional_namespace(token)?);

            let after = self.peek();

            match after.kind {
                TokenKind::Add => {
                    self.next();
                }
                TokenKind::CurlyOpen => {
                    break;
                }
                _ => {
                    error!(
                        after.location.clone(),
                        "expected a '+' or a '{{', found '{}' instead",
                        after.value
                    );
                }
            }
        }

        let location = SourceLocation::start_end(
            values.first().unwrap().location(),
            values.last().unwrap().location(),
        );

        Ok(Some(TypeNames { values, location }))
    }

    fn optional_type_parameter_requirements(
        &mut self,
    ) -> Result<Option<Requirements>, ParseError> {
        if self.peek().kind != TokenKind::Colon {
            return Ok(None);
        }

        Ok(Some(self.requirements()?))
    }

    fn reference_type(
        &mut self,
        start: Token,
    ) -> Result<ReferenceType, ParseError> {
        let type_token = self.require()?;
        let type_reference = match type_token.kind {
            TokenKind::Constant => {
                ReferrableType::Named(Box::new(self.type_name(type_token)?))
            }
            TokenKind::Identifier => ReferrableType::Named(Box::new(
                self.namespaced_type_name(type_token)?,
            )),
            TokenKind::Fn => ReferrableType::Closure(Box::new(
                self.closure_type(type_token)?,
            )),
            TokenKind::ParenOpen => {
                ReferrableType::Tuple(Box::new(self.tuple_type(type_token)?))
            }
            _ => error!(
                type_token.location,
                "expected a type name or 'fn'; found a '{}' instead",
                type_token.value
            ),
        };

        let location = SourceLocation::start_end(
            &start.location,
            type_reference.location(),
        );

        Ok(ReferenceType { type_reference, location })
    }

    fn tuple_type(&mut self, start: Token) -> Result<TupleType, ParseError> {
        let mut values = Vec::new();

        loop {
            let token = self.require()?;

            if token.kind == TokenKind::ParenClose {
                if values.is_empty() {
                    error!(
                        token.location,
                        "Tuple types must contain at least one member"
                    );
                }

                let location =
                    SourceLocation::start_end(&start.location, &token.location);

                return Ok(TupleType { values, location });
            }

            values.push(self.type_reference(token)?);

            if self.peek().kind == TokenKind::Comma {
                self.next();
            }
        }
    }

    fn closure_type(
        &mut self,
        start: Token,
    ) -> Result<ClosureType, ParseError> {
        let arguments = self.optional_block_argument_types()?;
        let return_type = self.optional_return_type()?;
        let end_loc = location!(return_type)
            .or_else(|| location!(arguments))
            .unwrap_or(&start.location);
        let location = SourceLocation::start_end(&start.location, end_loc);

        Ok(ClosureType { arguments, return_type, location })
    }

    fn optional_block_argument_types(
        &mut self,
    ) -> Result<Option<Types>, ParseError> {
        if self.peek().kind != TokenKind::ParenOpen {
            return Ok(None);
        }

        let (values, location) = self.list(
            TokenKind::ParenOpen,
            TokenKind::ParenClose,
            |parser, token| parser.type_reference(token),
        )?;

        Ok(Some(Types { values, location }))
    }

    fn optional_method_arguments(
        &mut self,
        allow_variadic: bool,
    ) -> Result<Option<MethodArguments>, ParseError> {
        if self.peek().kind != TokenKind::ParenOpen {
            return Ok(None);
        }

        let mut values = Vec::new();
        let mut variadic = false;
        let open_token = self.expect(TokenKind::ParenOpen)?;

        loop {
            let mut token = self.require()?;

            if allow_variadic && token.kind == TokenKind::Dot {
                self.expect(TokenKind::Dot)?;
                self.expect(TokenKind::Dot)?;

                if self.peek().kind == TokenKind::Comma {
                    self.next();
                }

                token = self.expect(TokenKind::ParenClose)?;
                variadic = true;
            }

            if token.kind == TokenKind::ParenClose {
                let location = SourceLocation::start_end(
                    &open_token.location,
                    &token.location,
                );

                return Ok(Some(MethodArguments {
                    values,
                    variadic,
                    location,
                }));
            }

            values.push(self.define_method_argument(token)?);

            if !values.is_empty() && self.peek().kind != TokenKind::ParenClose {
                self.expect(TokenKind::Comma)?;
            } else if self.peek().kind == TokenKind::Comma {
                self.next();
            }
        }
    }

    fn define_method_argument(
        &mut self,
        start: Token,
    ) -> Result<MethodArgument, ParseError> {
        let start_loc = start.location.clone();

        self.require_token_kind(&start, TokenKind::Identifier)?;
        self.expect(TokenKind::Colon)?;

        let name = Identifier::from(start);
        let type_token = self.require()?;
        let value_type = self.type_reference(type_token)?;
        let location =
            SourceLocation::start_end(&start_loc, value_type.location());

        Ok(MethodArgument { name, value_type, location })
    }

    fn optional_closure_arguments(
        &mut self,
    ) -> Result<Option<BlockArguments>, ParseError> {
        if self.peek().kind != TokenKind::ParenOpen {
            return Ok(None);
        }

        let (values, location) = self.list(
            TokenKind::ParenOpen,
            TokenKind::ParenClose,
            |parser, token| parser.define_closure_argument(token),
        )?;

        Ok(Some(BlockArguments { values, location }))
    }

    fn define_closure_argument(
        &mut self,
        start: Token,
    ) -> Result<BlockArgument, ParseError> {
        let start_loc = start.location.clone();

        self.require_token_kind(&start, TokenKind::Identifier)?;

        let name = Identifier::from(start);
        let value_type = if self.peek().kind == TokenKind::Colon {
            self.next();

            let type_token = self.require()?;

            Some(self.type_reference(type_token)?)
        } else {
            None
        };

        let end_loc = location!(value_type).unwrap_or_else(|| name.location());
        let location = SourceLocation::start_end(&start_loc, end_loc);

        Ok(BlockArgument { name, value_type, location })
    }

    fn optional_return_type(&mut self) -> Result<Option<Type>, ParseError> {
        if self.peek().kind != TokenKind::Arrow {
            return Ok(None);
        }

        self.next();

        let start = self.require()?;

        Ok(Some(self.type_reference(start)?))
    }

    fn define_module_method(
        &mut self,
        start: Token,
    ) -> Result<TopLevelExpression, ParseError> {
        let public = self.next_is_public();
        let mut allow_variadic = false;
        let kind = match self.peek().kind {
            TokenKind::Extern => {
                self.next();
                allow_variadic = true;
                MethodKind::Extern
            }
            _ => MethodKind::Instance,
        };

        let name_token = self.require()?;
        let (name, operator) = self.method_name(name_token)?;
        let type_parameters = if let MethodKind::Extern = kind {
            None
        } else {
            self.optional_type_parameter_definitions()?
        };
        let arguments = self.optional_method_arguments(allow_variadic)?;
        let variadic = arguments.as_ref().map_or(false, |v| v.variadic);
        let return_type = self.optional_return_type()?;
        let body = if (self.peek().kind == TokenKind::CurlyOpen
            || kind != MethodKind::Extern)
            && !variadic
        {
            let token = self.expect(TokenKind::CurlyOpen)?;

            Some(self.expressions(token)?)
        } else {
            None
        };

        let location = SourceLocation::start_end(
            &start.location,
            location!(body)
                .or_else(|| location!(return_type))
                .or_else(|| location!(arguments))
                .unwrap_or(&name.location),
        );

        Ok(TopLevelExpression::DefineMethod(Box::new(DefineMethod {
            public,
            operator,
            name,
            type_parameters,
            arguments,
            return_type,
            location,
            body,
            kind,
        })))
    }

    fn define_method(
        &mut self,
        start: Token,
    ) -> Result<DefineMethod, ParseError> {
        let public = self.next_is_public();
        let kind = match self.peek().kind {
            TokenKind::Async => {
                self.next();

                if self.peek().kind == TokenKind::Mut {
                    self.next();
                    MethodKind::AsyncMutable
                } else {
                    MethodKind::Async
                }
            }
            TokenKind::Move => {
                self.next();
                MethodKind::Moving
            }
            TokenKind::Static => {
                self.next();
                MethodKind::Static
            }
            TokenKind::Mut => {
                self.next();
                MethodKind::Mutable
            }
            _ => MethodKind::Instance,
        };
        let name_token = self.require()?;
        let (name, operator) = self.method_name(name_token)?;
        let type_parameters = self.optional_type_parameter_definitions()?;
        let arguments = self.optional_method_arguments(false)?;
        let return_type = self.optional_return_type()?;
        let body_token = self.expect(TokenKind::CurlyOpen)?;
        let body = self.expressions(body_token)?;
        let location =
            SourceLocation::start_end(&start.location, &body.location);

        Ok(DefineMethod {
            public,
            operator,
            name,
            type_parameters,
            arguments,
            return_type,
            location,
            body: Some(body),
            kind,
        })
    }

    fn implement_method(
        &mut self,
        start: Token,
    ) -> Result<DefineMethod, ParseError> {
        let public = self.next_is_public();
        let kind = match self.peek().kind {
            TokenKind::Move => {
                self.next();
                MethodKind::Moving
            }
            TokenKind::Mut => {
                self.next();
                MethodKind::Mutable
            }
            _ => MethodKind::Instance,
        };
        let name_token = self.require()?;
        let (name, operator) = self.method_name(name_token)?;
        let type_parameters = self.optional_type_parameter_definitions()?;
        let arguments = self.optional_method_arguments(false)?;
        let return_type = self.optional_return_type()?;
        let body_token = self.expect(TokenKind::CurlyOpen)?;
        let body = self.expressions(body_token)?;
        let location =
            SourceLocation::start_end(&start.location, &body.location);

        Ok(DefineMethod {
            public,
            operator,
            name,
            type_parameters,
            arguments,
            return_type,
            location,
            body: Some(body),
            kind,
        })
    }

    fn method_name(
        &mut self,
        start: Token,
    ) -> Result<(Identifier, bool), ParseError> {
        if start.is_operator() {
            return Ok((Identifier::from(start), true));
        }

        if start.kind == TokenKind::Identifier
            || start.kind == TokenKind::Constant
            || start.kind == TokenKind::Integer
            || start.is_keyword()
        {
            let mut name = start.value;
            let mut location = start.location;

            if self.peek().kind == TokenKind::Assign {
                let assign = self.next();

                name.push('=');

                location =
                    SourceLocation::start_end(&location, &assign.location);
            }

            Ok((Identifier { name, location }, false))
        } else {
            error!(
                start.location,
                "expected an identifier or constant, found '{}' instead",
                start.value
            );
        }
    }

    fn define_class(
        &mut self,
        start: Token,
    ) -> Result<TopLevelExpression, ParseError> {
        let public = self.next_is_public();
        let kind = match self.peek().kind {
            TokenKind::Async => {
                self.next();
                ClassKind::Async
            }
            TokenKind::Enum => {
                self.next();
                ClassKind::Enum
            }
            TokenKind::Builtin => {
                self.next();
                ClassKind::Builtin
            }
            TokenKind::Extern => {
                self.next();
                ClassKind::Extern
            }
            _ => ClassKind::Regular,
        };

        let name = Constant::from(self.expect(TokenKind::Constant)?);
        let type_parameters = if let ClassKind::Extern = kind {
            None
        } else {
            self.optional_type_parameter_definitions()?
        };

        let body = if let ClassKind::Extern = kind {
            self.extern_class_expressions()?
        } else {
            self.class_expressions()?
        };

        let location =
            SourceLocation::start_end(&start.location, &body.location);

        Ok(TopLevelExpression::DefineClass(Box::new(DefineClass {
            public,
            kind,
            name,
            type_parameters,
            body,
            location,
        })))
    }

    fn define_variant(
        &mut self,
        start: Token,
    ) -> Result<DefineVariant, ParseError> {
        let name = Constant::from(self.expect(TokenKind::Constant)?);
        let members = if self.peek().kind == TokenKind::ParenOpen {
            let (values, location) = self.list(
                TokenKind::ParenOpen,
                TokenKind::ParenClose,
                |parser, token| parser.type_reference(token),
            )?;

            Some(Types { values, location })
        } else {
            None
        };

        let end_loc =
            members.as_ref().map(|n| &n.location).unwrap_or(&start.location);
        let location = SourceLocation::start_end(&start.location, end_loc);

        Ok(DefineVariant { name, members, location })
    }

    fn class_expressions(&mut self) -> Result<ClassExpressions, ParseError> {
        let start = self.expect(TokenKind::CurlyOpen)?;
        let mut values = Vec::new();

        loop {
            let token = self.require()?;

            if token.kind == TokenKind::CurlyClose {
                let location =
                    SourceLocation::start_end(&start.location, &token.location);

                return Ok(ClassExpressions { values, location });
            }

            values.push(self.class_expression(token)?);
        }
    }

    fn extern_class_expressions(
        &mut self,
    ) -> Result<ClassExpressions, ParseError> {
        let start = self.expect(TokenKind::CurlyOpen)?;
        let mut values = Vec::new();

        loop {
            let token = self.require()?;

            if token.kind == TokenKind::CurlyClose {
                let location =
                    SourceLocation::start_end(&start.location, &token.location);

                return Ok(ClassExpressions { values, location });
            }

            let node = match token.kind {
                TokenKind::Let => ClassExpression::DefineField(Box::new(
                    self.define_field(token)?,
                )),
                TokenKind::Comment => {
                    ClassExpression::Comment(self.comment(token))
                }
                _ => {
                    error!(
                        token.location,
                        "expected a 'let', found '{}' instead", token.value
                    );
                }
            };

            values.push(node);
        }
    }

    fn class_expression(
        &mut self,
        start: Token,
    ) -> Result<ClassExpression, ParseError> {
        let expr = match start.kind {
            TokenKind::Let => ClassExpression::DefineField(Box::new(
                self.define_field(start)?,
            )),
            TokenKind::Fn => ClassExpression::DefineMethod(Box::new(
                self.define_method(start)?,
            )),
            TokenKind::Case => ClassExpression::DefineVariant(Box::new(
                self.define_variant(start)?,
            )),
            TokenKind::Comment => ClassExpression::Comment(self.comment(start)),
            _ => {
                error!(
                    start.location,
                    "expected 'fn', 'let' or 'case', found '{}' instead",
                    start.value
                );
            }
        };

        Ok(expr)
    }

    fn define_field(
        &mut self,
        start: Token,
    ) -> Result<DefineField, ParseError> {
        let public = self.next_is_public();
        let name = Identifier::from(self.expect(TokenKind::Field)?);

        self.expect(TokenKind::Colon)?;

        let value_type_token = self.require()?;
        let value_type = self.type_reference(value_type_token)?;
        let location =
            SourceLocation::start_end(&start.location, value_type.location());

        Ok(DefineField { name, public, value_type, location })
    }

    fn implementation(
        &mut self,
        start: Token,
    ) -> Result<TopLevelExpression, ParseError> {
        let token = self.expect(TokenKind::Constant)?;
        let peeked = self.peek();

        if peeked.kind == TokenKind::For
            || peeked.kind == TokenKind::BracketOpen
        {
            self.implement_trait(start, token)
        } else {
            self.reopen_class(start, token)
        }
    }

    fn implement_trait(
        &mut self,
        start: Token,
        trait_token: Token,
    ) -> Result<TopLevelExpression, ParseError> {
        let trait_name = self.type_name(trait_token)?;

        self.expect(TokenKind::For)?;

        let class_name = Constant::from(self.expect(TokenKind::Constant)?);
        let bounds = self.optional_type_bounds()?;
        let body = self.trait_implementation_expressions()?;
        let location =
            SourceLocation::start_end(&start.location, body.location());

        Ok(TopLevelExpression::ImplementTrait(Box::new(ImplementTrait {
            trait_name,
            class_name,
            body,
            location,
            bounds,
        })))
    }

    fn optional_type_bounds(
        &mut self,
    ) -> Result<Option<TypeBounds>, ParseError> {
        if self.peek().kind != TokenKind::If {
            return Ok(None);
        }

        self.next();

        let mut values = Vec::new();

        loop {
            if self.peek().kind == TokenKind::Constant {
                let token = self.require()?;

                values.push(self.type_bound(token)?);
            } else {
                break;
            }

            if self.peek().kind == TokenKind::Comma {
                self.next();
            } else {
                break;
            }
        }

        let start_loc = values.first().unwrap().location();
        let end_loc = values.last().unwrap().location();
        let location = SourceLocation::start_end(start_loc, end_loc);

        Ok(Some(TypeBounds { values, location }))
    }

    fn type_bound(&mut self, start: Token) -> Result<TypeBound, ParseError> {
        let name = Constant::from(start);
        let requirements = self.requirements()?;
        let location =
            SourceLocation::start_end(name.location(), requirements.location());

        Ok(TypeBound { name, requirements, location })
    }

    fn requirements(&mut self) -> Result<Requirements, ParseError> {
        self.expect(TokenKind::Colon)?;

        let mut values = Vec::new();

        loop {
            let token = self.require()?;
            let req = match token.kind {
                TokenKind::Mut => Requirement::Mutable(token.location),
                _ => Requirement::Trait(
                    self.type_name_with_optional_namespace(token)?,
                ),
            };

            values.push(req);

            let after = self.peek();

            match after.kind {
                TokenKind::Add => {
                    self.next();
                }
                _ => {
                    break;
                }
            }
        }

        let location = SourceLocation::start_end(
            values.first().unwrap().location(),
            values.last().unwrap().location(),
        );

        Ok(Requirements { values, location })
    }

    fn reopen_class(
        &mut self,
        start: Token,
        class_token: Token,
    ) -> Result<TopLevelExpression, ParseError> {
        let class_name = Constant::from(class_token);
        let bounds = self.optional_type_bounds()?;
        let body = self.reopen_class_expressions()?;
        let end_loc = location!(bounds).unwrap_or_else(|| body.location());
        let location = SourceLocation::start_end(&start.location, end_loc);

        Ok(TopLevelExpression::ReopenClass(Box::new(ReopenClass {
            class_name,
            body,
            location,
            bounds,
        })))
    }

    fn reopen_class_expressions(
        &mut self,
    ) -> Result<ImplementationExpressions, ParseError> {
        let start = self.expect(TokenKind::CurlyOpen)?;
        let mut values = Vec::new();

        loop {
            let token = self.require()?;

            if token.kind == TokenKind::CurlyClose {
                let location =
                    SourceLocation::start_end(&start.location, &token.location);

                return Ok(ImplementationExpressions { values, location });
            }

            let value = match token.kind {
                TokenKind::Fn => ImplementationExpression::DefineMethod(
                    Box::new(self.define_method(token)?),
                ),
                TokenKind::Comment => {
                    ImplementationExpression::Comment(self.comment(token))
                }
                _ => error!(
                    token.location,
                    "expected a method, found '{}' instead", token.value
                ),
            };

            values.push(value);
        }
    }

    fn trait_implementation_expressions(
        &mut self,
    ) -> Result<ImplementationExpressions, ParseError> {
        let start = self.expect(TokenKind::CurlyOpen)?;
        let mut values = Vec::new();

        loop {
            let token = self.require()?;

            if token.kind == TokenKind::CurlyClose {
                let location =
                    SourceLocation::start_end(&start.location, &token.location);

                return Ok(ImplementationExpressions { values, location });
            }

            let value = match token.kind {
                TokenKind::Fn => ImplementationExpression::DefineMethod(
                    Box::new(self.implement_method(token)?),
                ),
                TokenKind::Comment => {
                    ImplementationExpression::Comment(self.comment(token))
                }
                _ => error!(
                    token.location,
                    "expected a method, found '{}' instead", token.value
                ),
            };

            values.push(value);
        }
    }

    fn define_trait(
        &mut self,
        start: Token,
    ) -> Result<TopLevelExpression, ParseError> {
        let public = self.next_is_public();
        let name = Constant::from(self.expect(TokenKind::Constant)?);
        let type_parameters = self.optional_type_parameter_definitions()?;
        let requirements = self.optional_trait_requirements()?;
        let body = self.trait_expressions()?;
        let location =
            SourceLocation::start_end(&start.location, &body.location);

        Ok(TopLevelExpression::DefineTrait(Box::new(DefineTrait {
            public,
            name,
            type_parameters,
            requirements,
            body,
            location,
        })))
    }

    fn trait_expressions(&mut self) -> Result<TraitExpressions, ParseError> {
        let start = self.expect(TokenKind::CurlyOpen)?;
        let mut values = Vec::new();

        loop {
            let token = self.require()?;

            if token.kind == TokenKind::CurlyClose {
                let location =
                    SourceLocation::start_end(&start.location, &token.location);

                return Ok(TraitExpressions { values, location });
            }

            let expr = match token.kind {
                TokenKind::Move | TokenKind::Fn => {
                    TraitExpression::DefineMethod(Box::new(
                        self.define_trait_method(token)?,
                    ))
                }
                TokenKind::Comment => {
                    TraitExpression::Comment(self.comment(token))
                }
                _ => error!(
                    token.location,
                    "expected a method, found '{}' instead", token.value
                ),
            };

            values.push(expr);
        }
    }

    fn define_trait_method(
        &mut self,
        start: Token,
    ) -> Result<DefineMethod, ParseError> {
        let public = self.next_is_public();
        let kind = match self.peek().kind {
            TokenKind::Move => {
                self.next();
                MethodKind::Moving
            }
            TokenKind::Mut => {
                self.next();
                MethodKind::Mutable
            }
            _ => MethodKind::Instance,
        };
        let name_token = self.require()?;
        let (name, operator) = self.method_name(name_token)?;
        let type_parameters = self.optional_type_parameter_definitions()?;
        let arguments = self.optional_method_arguments(false)?;
        let return_type = self.optional_return_type()?;
        let body = if self.peek().kind == TokenKind::CurlyOpen {
            let body_token = self.expect(TokenKind::CurlyOpen)?;

            Some(self.expressions(body_token)?)
        } else {
            None
        };
        let end_loc = location!(body)
            .or_else(|| location!(return_type))
            .or_else(|| location!(arguments))
            .or_else(|| location!(type_parameters))
            .unwrap_or_else(|| name.location());
        let location = SourceLocation::start_end(&start.location, end_loc);

        Ok(DefineMethod {
            public,
            operator,
            name,
            type_parameters,
            arguments,
            return_type,
            location,
            body,
            kind,
        })
    }

    fn expressions(&mut self, start: Token) -> Result<Expressions, ParseError> {
        let mut values = Vec::new();

        loop {
            let token = self.require()?;

            if token.kind == TokenKind::CurlyClose {
                let location =
                    SourceLocation::start_end(&start.location, &token.location);

                return Ok(Expressions { values, location });
            }

            values.push(self.expression(token)?);
        }
    }

    fn expressions_with_optional_curly_braces(
        &mut self,
    ) -> Result<Expressions, ParseError> {
        let start = self.require()?;

        if start.kind == TokenKind::CurlyOpen {
            self.expressions(start)
        } else {
            let expr = self.expression(start)?;
            let location = expr.location().clone();

            Ok(Expressions { values: vec![expr], location })
        }
    }

    fn expression(&mut self, start: Token) -> Result<Expression, ParseError> {
        self.boolean_and_or(start, true)
    }

    fn expression_without_trailing_block(
        &mut self,
    ) -> Result<Expression, ParseError> {
        let start = self.require()?;

        self.boolean_and_or(start, false)
    }

    fn boolean_and_or(
        &mut self,
        start: Token,
        trailing: bool,
    ) -> Result<Expression, ParseError> {
        let mut node = self.binary(start, trailing)?;

        loop {
            match self.peek().kind {
                TokenKind::And => {
                    self.next();

                    let right_token = self.require()?;
                    let right = self.binary(right_token, trailing)?;

                    node = Expression::boolean_and(node, right);
                }
                TokenKind::Or => {
                    self.next();

                    let right_token = self.require()?;
                    let right = self.binary(right_token, trailing)?;

                    node = Expression::boolean_or(node, right);
                }
                _ => break,
            }
        }

        Ok(node)
    }

    fn binary(
        &mut self,
        start: Token,
        trailing: bool,
    ) -> Result<Expression, ParseError> {
        let mut node = self.postfix(start, trailing)?;

        loop {
            if let Some(op) = self.binary_operator() {
                let rhs_token = self.require()?;
                let rhs = self.postfix(rhs_token, trailing)?;
                let location =
                    SourceLocation::start_end(node.location(), rhs.location());

                node = Expression::Binary(Box::new(Binary {
                    operator: op,
                    left: node,
                    right: rhs,
                    location,
                }));
            } else if self.peek().kind == TokenKind::As {
                self.next();

                let cast_token = self.require()?;
                let cast_to = self.type_reference(cast_token)?;
                let location = SourceLocation::start_end(
                    node.location(),
                    cast_to.location(),
                );

                node = Expression::TypeCast(Box::new(TypeCast {
                    value: node,
                    cast_to,
                    location,
                }));
            } else {
                break;
            }
        }

        Ok(node)
    }

    fn binary_operator(&mut self) -> Option<Operator> {
        let op_kind = match self.peek().kind {
            TokenKind::Add => OperatorKind::Add,
            TokenKind::Sub => OperatorKind::Sub,
            TokenKind::Div => OperatorKind::Div,
            TokenKind::Mul => OperatorKind::Mul,
            TokenKind::Pow => OperatorKind::Pow,
            TokenKind::Mod => OperatorKind::Mod,
            TokenKind::Lt => OperatorKind::Lt,
            TokenKind::Gt => OperatorKind::Gt,
            TokenKind::Le => OperatorKind::Le,
            TokenKind::Ge => OperatorKind::Ge,
            TokenKind::Shl => OperatorKind::Shl,
            TokenKind::Shr => OperatorKind::Shr,
            TokenKind::UnsignedShr => OperatorKind::UnsignedShr,
            TokenKind::BitAnd => OperatorKind::BitAnd,
            TokenKind::BitOr => OperatorKind::BitOr,
            TokenKind::BitXor => OperatorKind::BitXor,
            TokenKind::Eq => OperatorKind::Eq,
            TokenKind::Ne => OperatorKind::Ne,
            _ => return None,
        };

        let op_token = self.next();

        Some(Operator { kind: op_kind, location: op_token.location })
    }

    fn postfix(
        &mut self,
        start: Token,
        trailing: bool,
    ) -> Result<Expression, ParseError> {
        let mut node = self.value(start, trailing)?;

        loop {
            let peeked = self.peek();

            if let TokenKind::Dot = peeked.kind {
                node = self.call_with_receiver(node)?;
            } else {
                break;
            }
        }

        Ok(node)
    }

    fn value(
        &mut self,
        start: Token,
        trailing: bool,
    ) -> Result<Expression, ParseError> {
        // When updating this match, also update the one used for parsing return
        // value expressions.
        let value = match start.kind {
            TokenKind::BracketOpen => self.array_literal(start)?,
            TokenKind::Break => self.break_loop(start),
            TokenKind::Constant => self.constant(start, trailing)?,
            TokenKind::CurlyOpen => self.scope(start)?,
            TokenKind::Fn => self.closure(start)?,
            TokenKind::SingleStringOpen => {
                self.string_value(start, TokenKind::SingleStringClose, true)?
            }
            TokenKind::DoubleStringOpen => {
                self.string_value(start, TokenKind::DoubleStringClose, true)?
            }
            TokenKind::False => self.false_literal(start),
            TokenKind::Field => self.field(start)?,
            TokenKind::Float => self.float_literal(start),
            TokenKind::Identifier => self.identifier(start)?,
            TokenKind::If => self.if_expression(start)?,
            TokenKind::Integer => self.int_literal(start),
            TokenKind::Loop => self.loop_expression(start)?,
            TokenKind::Match => self.match_expression(start)?,
            TokenKind::Next => self.next_loop(start),
            TokenKind::ParenOpen => self.group_or_tuple(start)?,
            TokenKind::Ref => self.reference(start)?,
            TokenKind::Mut => self.mut_reference(start)?,
            TokenKind::Recover => self.recover_expression(start)?,
            TokenKind::Return => self.return_expression(start)?,
            TokenKind::SelfObject => self.self_expression(start),
            TokenKind::Throw => self.throw_expression(start)?,
            TokenKind::True => self.true_literal(start),
            TokenKind::Nil => self.nil_literal(start),
            TokenKind::Try => self.try_expression(start)?,
            TokenKind::While => self.while_expression(start)?,
            TokenKind::Let => self.define_variable(start)?,
            TokenKind::Comment => Expression::Comment(self.comment(start)),
            _ => {
                error!(start.location, "'{}' can't be used here", start.value)
            }
        };

        Ok(value)
    }

    fn int_literal(&self, start: Token) -> Expression {
        Expression::Int(Box::new(IntLiteral {
            value: start.value,
            location: start.location,
        }))
    }

    fn float_literal(&mut self, start: Token) -> Expression {
        Expression::Float(Box::new(FloatLiteral {
            value: start.value,
            location: start.location,
        }))
    }

    fn string_value(
        &mut self,
        start: Token,
        close: TokenKind,
        interpolation: bool,
    ) -> Result<Expression, ParseError> {
        Ok(Expression::String(Box::new(self.string_literal(
            start,
            close,
            interpolation,
        )?)))
    }

    fn string_literal(
        &mut self,
        start: Token,
        close: TokenKind,
        interpolation: bool,
    ) -> Result<StringLiteral, ParseError> {
        let mut values = Vec::new();

        loop {
            let token = self.require()?;

            match token.kind {
                TokenKind::StringText => {
                    values.push(StringValue::Text(Box::new(
                        self.string_text(token),
                    )));
                }
                TokenKind::StringEscape => {
                    values.push(StringValue::Escape(self.string_escape(token)));
                }
                TokenKind::StringExprOpen if interpolation => {
                    let value_token = self.require()?;
                    let value = self.expression(value_token)?;
                    let close = self.expect(TokenKind::StringExprClose)?;
                    let location = SourceLocation::start_end(
                        &token.location,
                        &close.location,
                    );

                    values.push(StringValue::Expression(Box::new(
                        StringExpression { value, location },
                    )));
                }
                TokenKind::StringExprOpen => {
                    error!(
                        token.location,
                        "this string doesn't support string interpolation"
                    )
                }
                TokenKind::InvalidStringEscape => {
                    error!(
                        token.location,
                        "the escape sequence '\\{}' is invalid", token.value
                    );
                }
                kind if kind == close => {
                    let location = SourceLocation::start_end(
                        &start.location,
                        &token.location,
                    );

                    return Ok(StringLiteral { values, location });
                }
                _ => {
                    error!(
                        token.location,
                        "expected the text of a String, an expression, \
                        a single qoute, or a double quote, found '{}' instead",
                        token.value
                    );
                }
            }
        }
    }

    #[allow(clippy::redundant_clone)]
    fn string_text(&mut self, start: Token) -> StringText {
        let mut value = start.value;
        let mut end_loc = start.location.clone();

        while matches!(self.peek().kind, TokenKind::StringText) {
            let token = self.next();

            value += &token.value;
            end_loc = token.location;
        }

        let location = SourceLocation::start_end(&start.location, &end_loc);

        StringText { value, location }
    }

    fn string_escape(&self, start: Token) -> Box<StringEscape> {
        Box::new(StringEscape { value: start.value, location: start.location })
    }

    fn array_literal(
        &mut self,
        start: Token,
    ) -> Result<Expression, ParseError> {
        let mut values = Vec::new();

        loop {
            let token = self.require()?;

            if token.kind == TokenKind::BracketClose {
                let location =
                    SourceLocation::start_end(&start.location, &token.location);

                return Ok(Expression::Array(Box::new(Array {
                    values,
                    location,
                })));
            }

            values.push(self.expression(token)?);

            if self.peek().kind == TokenKind::Comma {
                self.next();
            }
        }
    }

    fn field(&mut self, start: Token) -> Result<Expression, ParseError> {
        match self.peek().kind {
            TokenKind::Assign => return self.assign_field(start),
            TokenKind::Replace => return self.replace_field(start),
            TokenKind::AddAssign => {
                return self.binary_assign_field(start, OperatorKind::Add)
            }
            TokenKind::SubAssign => {
                return self.binary_assign_field(start, OperatorKind::Sub)
            }
            TokenKind::DivAssign => {
                return self.binary_assign_field(start, OperatorKind::Div)
            }
            TokenKind::MulAssign => {
                return self.binary_assign_field(start, OperatorKind::Mul)
            }
            TokenKind::PowAssign => {
                return self.binary_assign_field(start, OperatorKind::Pow)
            }
            TokenKind::ModAssign => {
                return self.binary_assign_field(start, OperatorKind::Mod)
            }
            TokenKind::ShlAssign => {
                return self.binary_assign_field(start, OperatorKind::Shl)
            }
            TokenKind::ShrAssign => {
                return self.binary_assign_field(start, OperatorKind::Shr)
            }
            TokenKind::UnsignedShrAssign => {
                return self
                    .binary_assign_field(start, OperatorKind::UnsignedShr)
            }
            TokenKind::BitOrAssign => {
                return self.binary_assign_field(start, OperatorKind::BitOr)
            }
            TokenKind::BitAndAssign => {
                return self.binary_assign_field(start, OperatorKind::BitAnd)
            }
            TokenKind::BitXorAssign => {
                return self.binary_assign_field(start, OperatorKind::BitXor)
            }
            _ => {}
        }

        Ok(Expression::Field(Box::new(Field::from(start))))
    }

    fn constant(
        &mut self,
        start: Token,
        trailing: bool,
    ) -> Result<Expression, ParseError> {
        if let Some(args) = self.arguments(&start.location)? {
            let name = Identifier::from(start);
            let location =
                SourceLocation::start_end(name.location(), args.location());

            return Ok(Expression::Call(Box::new(Call {
                receiver: None,
                name,
                arguments: Some(args),
                location,
            })));
        }

        let peeked = self.peek();
        let same_line = peeked.same_line_as(&start);
        let name = Constant::from(start);

        if peeked.kind == TokenKind::CurlyOpen && same_line && trailing {
            return self.class_literal(name);
        }

        Ok(Expression::Constant(Box::new(name)))
    }

    fn class_literal(
        &mut self,
        class_name: Constant,
    ) -> Result<Expression, ParseError> {
        let (fields, location) = self.list(
            TokenKind::CurlyOpen,
            TokenKind::CurlyClose,
            |parser, token| {
                parser.require_token_kind(&token, TokenKind::Field)?;
                parser.expect(TokenKind::Assign)?;

                let value_token = parser.require()?;
                let value = parser.expression(value_token)?;
                let field = Field::from(token);
                let location = SourceLocation::start_end(
                    field.location(),
                    value.location(),
                );

                Ok(AssignInstanceLiteralField { field, value, location })
            },
        )?;

        let location =
            SourceLocation::start_end(class_name.location(), &location);

        Ok(Expression::ClassLiteral(Box::new(ClassLiteral {
            class_name,
            fields,
            location,
        })))
    }

    fn identifier(&mut self, start: Token) -> Result<Expression, ParseError> {
        match self.peek().kind {
            TokenKind::Assign => return self.assign_variable(start),
            TokenKind::Replace => return self.replace_variable(start),
            TokenKind::AddAssign => {
                return self.binary_assign_variable(start, OperatorKind::Add)
            }
            TokenKind::SubAssign => {
                return self.binary_assign_variable(start, OperatorKind::Sub)
            }
            TokenKind::DivAssign => {
                return self.binary_assign_variable(start, OperatorKind::Div)
            }
            TokenKind::MulAssign => {
                return self.binary_assign_variable(start, OperatorKind::Mul)
            }
            TokenKind::PowAssign => {
                return self.binary_assign_variable(start, OperatorKind::Pow)
            }
            TokenKind::ModAssign => {
                return self.binary_assign_variable(start, OperatorKind::Mod)
            }
            TokenKind::ShlAssign => {
                return self.binary_assign_variable(start, OperatorKind::Shl)
            }
            TokenKind::ShrAssign => {
                return self.binary_assign_variable(start, OperatorKind::Shr);
            }
            TokenKind::UnsignedShrAssign => {
                return self
                    .binary_assign_variable(start, OperatorKind::UnsignedShr);
            }
            TokenKind::BitOrAssign => {
                return self.binary_assign_variable(start, OperatorKind::BitOr)
            }
            TokenKind::BitAndAssign => {
                return self.binary_assign_variable(start, OperatorKind::BitAnd)
            }
            TokenKind::BitXorAssign => {
                return self.binary_assign_variable(start, OperatorKind::BitXor)
            }
            _ => {}
        }

        if let Some(args) = self.arguments(&start.location)? {
            let name = Identifier::from(start);
            let location =
                SourceLocation::start_end(name.location(), args.location());

            return Ok(Expression::Call(Box::new(Call {
                receiver: None,
                name,
                arguments: Some(args),
                location,
            })));
        }

        Ok(Expression::Identifier(Box::new(Identifier::from(start))))
    }

    fn trailing_block_argument(
        &mut self,
        start_location: &SourceLocation,
    ) -> Result<Option<Argument>, ParseError> {
        let peeked = self.peek();

        // Trailing blocks are only treated as an argument if they occur on the
        // same line as the call or its arguments.
        if peeked.location.lines.start() > start_location.lines.end() {
            return Ok(None);
        }

        let value = match peeked.kind {
            TokenKind::Fn => {
                let start = self.next();

                self.closure(start)?
            }
            _ => {
                return Ok(None);
            }
        };

        Ok(Some(Argument::Positional(value)))
    }

    fn arguments(
        &mut self,
        start_location: &SourceLocation,
    ) -> Result<Option<Arguments>, ParseError> {
        if let Some(block) = self.trailing_block_argument(start_location)? {
            let location = block.location().clone();

            return Ok(Some(Arguments { values: vec![block], location }));
        }

        let peeked = self.peek();

        if peeked.kind != TokenKind::ParenOpen
            || peeked.location.lines.start() != start_location.lines.start()
        {
            return Ok(None);
        }

        let mut allow_pos = true;
        let (mut values, location) = self.list(
            TokenKind::ParenOpen,
            TokenKind::ParenClose,
            |parser, token| {
                let node = if (token.kind == TokenKind::Identifier
                    || token.is_keyword())
                    && parser.peek().kind == TokenKind::Colon
                {
                    allow_pos = false;
                    Argument::Named(Box::new(parser.named_argument(token)?))
                } else if allow_pos {
                    Argument::Positional(parser.expression(token)?)
                } else {
                    error!(
                        token.location,
                        "expected a named argument, found '{}' instead",
                        token.value
                    );
                };

                Ok(node)
            },
        )?;

        if let Some(block) = self.trailing_block_argument(&location)? {
            values.push(block);
        }

        Ok(Some(Arguments { values, location }))
    }

    fn call_with_receiver(
        &mut self,
        receiver: Expression,
    ) -> Result<Expression, ParseError> {
        self.next();

        let name_token = self.require()?;

        match name_token.kind {
            TokenKind::Identifier
            | TokenKind::Constant
            | TokenKind::Integer => {}
            _ if name_token.is_keyword() => {}
            _ => {
                error!(
                    name_token.location,
                    "expected an identifier or keyword, found '{}' instead",
                    name_token.value
                );
            }
        };

        match self.peek().kind {
            TokenKind::Assign => {
                return self.assign_setter(receiver, name_token);
            }
            TokenKind::AddAssign => {
                return self.binary_assign_setter(
                    receiver,
                    name_token,
                    OperatorKind::Add,
                );
            }
            TokenKind::SubAssign => {
                return self.binary_assign_setter(
                    receiver,
                    name_token,
                    OperatorKind::Sub,
                );
            }
            TokenKind::DivAssign => {
                return self.binary_assign_setter(
                    receiver,
                    name_token,
                    OperatorKind::Div,
                );
            }
            TokenKind::MulAssign => {
                return self.binary_assign_setter(
                    receiver,
                    name_token,
                    OperatorKind::Mul,
                );
            }
            TokenKind::PowAssign => {
                return self.binary_assign_setter(
                    receiver,
                    name_token,
                    OperatorKind::Pow,
                );
            }
            TokenKind::ModAssign => {
                return self.binary_assign_setter(
                    receiver,
                    name_token,
                    OperatorKind::Mod,
                );
            }
            TokenKind::ShlAssign => {
                return self.binary_assign_setter(
                    receiver,
                    name_token,
                    OperatorKind::Shl,
                );
            }
            TokenKind::ShrAssign => {
                return self.binary_assign_setter(
                    receiver,
                    name_token,
                    OperatorKind::Shr,
                );
            }
            TokenKind::UnsignedShrAssign => {
                return self.binary_assign_setter(
                    receiver,
                    name_token,
                    OperatorKind::UnsignedShr,
                );
            }
            TokenKind::BitOrAssign => {
                return self.binary_assign_setter(
                    receiver,
                    name_token,
                    OperatorKind::BitOr,
                );
            }
            TokenKind::BitAndAssign => {
                return self.binary_assign_setter(
                    receiver,
                    name_token,
                    OperatorKind::BitAnd,
                );
            }
            TokenKind::BitXorAssign => {
                return self.binary_assign_setter(
                    receiver,
                    name_token,
                    OperatorKind::BitXor,
                );
            }
            _ => {}
        }

        let name = Identifier::from(name_token);
        let arguments = self.arguments(name.location())?;
        let end_loc = location!(arguments).unwrap_or_else(|| name.location());
        let location = SourceLocation::start_end(receiver.location(), end_loc);

        Ok(Expression::Call(Box::new(Call {
            receiver: Some(receiver),
            name,
            arguments,
            location,
        })))
    }

    fn assign_setter(
        &mut self,
        receiver: Expression,
        name_token: Token,
    ) -> Result<Expression, ParseError> {
        self.next();

        let name = Identifier::from(name_token);
        let value_token = self.require()?;
        let value = self.expression(value_token)?;
        let location =
            SourceLocation::start_end(receiver.location(), value.location());

        Ok(Expression::AssignSetter(Box::new(AssignSetter {
            receiver,
            name,
            value,
            location,
        })))
    }

    fn binary_assign_setter(
        &mut self,
        receiver: Expression,
        name_token: Token,
        kind: OperatorKind,
    ) -> Result<Expression, ParseError> {
        let setter = Identifier::from(name_token);
        let op_tok = self.next();
        let operator = Operator { kind, location: op_tok.location };
        let value_token = self.require()?;
        let value = self.expression(value_token)?;
        let location =
            SourceLocation::start_end(receiver.location(), value.location());

        Ok(Expression::BinaryAssignSetter(Box::new(BinaryAssignSetter {
            operator,
            receiver,
            name: setter,
            value,
            location,
        })))
    }

    fn named_argument(
        &mut self,
        start: Token,
    ) -> Result<NamedArgument, ParseError> {
        self.next();

        let name = Identifier::from(start);
        let value_token = self.require()?;
        let value = self.expression(value_token)?;
        let location =
            SourceLocation::start_end(name.location(), value.location());

        Ok(NamedArgument { name, value, location })
    }

    fn assign_variable(
        &mut self,
        start: Token,
    ) -> Result<Expression, ParseError> {
        self.next();

        let variable = Identifier::from(start);
        let value_token = self.require()?;
        let value = self.expression(value_token)?;
        let location =
            SourceLocation::start_end(variable.location(), value.location());

        Ok(Expression::AssignVariable(Box::new(AssignVariable {
            variable,
            value,
            location,
        })))
    }

    fn replace_variable(
        &mut self,
        start: Token,
    ) -> Result<Expression, ParseError> {
        self.next();

        let variable = Identifier::from(start);
        let value_token = self.require()?;
        let value = self.expression(value_token)?;
        let location =
            SourceLocation::start_end(variable.location(), value.location());

        Ok(Expression::ReplaceVariable(Box::new(ReplaceVariable {
            variable,
            value,
            location,
        })))
    }

    fn assign_field(&mut self, start: Token) -> Result<Expression, ParseError> {
        self.next();

        let field = Field::from(start);
        let value_token = self.require()?;
        let value = self.expression(value_token)?;
        let location =
            SourceLocation::start_end(field.location(), value.location());

        Ok(Expression::AssignField(Box::new(AssignField {
            field,
            value,
            location,
        })))
    }

    fn replace_field(
        &mut self,
        start: Token,
    ) -> Result<Expression, ParseError> {
        self.next();

        let field = Field::from(start);
        let value_token = self.require()?;
        let value = self.expression(value_token)?;
        let location =
            SourceLocation::start_end(field.location(), value.location());

        Ok(Expression::ReplaceField(Box::new(ReplaceField {
            field,
            value,
            location,
        })))
    }

    fn scope(&mut self, start: Token) -> Result<Expression, ParseError> {
        let body = self.expressions(start)?;
        let location = body.location.clone();

        Ok(Expression::Scope(Box::new(Scope { body, location })))
    }

    fn closure(&mut self, start: Token) -> Result<Expression, ParseError> {
        let moving = if self.peek().kind == TokenKind::Move {
            self.next();
            true
        } else {
            false
        };
        let arguments = self.optional_closure_arguments()?;
        let return_type = self.optional_return_type()?;
        let body_token = self.expect(TokenKind::CurlyOpen)?;
        let body = self.expressions(body_token)?;
        let location =
            SourceLocation::start_end(&start.location, &body.location);
        let closure =
            Closure { moving, body, arguments, return_type, location };

        Ok(Expression::Closure(Box::new(closure)))
    }

    fn binary_assign_variable(
        &mut self,
        start: Token,
        kind: OperatorKind,
    ) -> Result<Expression, ParseError> {
        let op_tok = self.next();
        let operator = Operator { kind, location: op_tok.location };
        let variable = Identifier::from(start);
        let value_token = self.require()?;
        let value = self.expression(value_token)?;
        let location =
            SourceLocation::start_end(variable.location(), value.location());

        Ok(Expression::BinaryAssignVariable(Box::new(BinaryAssignVariable {
            operator,
            variable,
            value,
            location,
        })))
    }

    fn binary_assign_field(
        &mut self,
        start: Token,
        kind: OperatorKind,
    ) -> Result<Expression, ParseError> {
        let op_tok = self.next();
        let operator = Operator { kind, location: op_tok.location };
        let field = Field::from(start);
        let value_token = self.require()?;
        let value = self.expression(value_token)?;
        let location =
            SourceLocation::start_end(field.location(), value.location());

        Ok(Expression::BinaryAssignField(Box::new(BinaryAssignField {
            operator,
            field,
            value,
            location,
        })))
    }

    fn define_variable(
        &mut self,
        start: Token,
    ) -> Result<Expression, ParseError> {
        let mutable = if self.peek().kind == TokenKind::Mut {
            self.next();
            true
        } else {
            false
        };

        let name = Identifier::from(self.expect(TokenKind::Identifier)?);
        let value_type = self.optional_type_annotation()?;

        self.expect(TokenKind::Assign)?;

        let value_start = self.require()?;
        let value = self.expression(value_start)?;
        let location =
            SourceLocation::start_end(&start.location, value.location());

        Ok(Expression::DefineVariable(Box::new(DefineVariable {
            mutable,
            name,
            value_type,
            value,
            location,
        })))
    }

    fn self_expression(&mut self, start: Token) -> Expression {
        Expression::SelfObject(Box::new(SelfObject {
            location: start.location,
        }))
    }

    fn true_literal(&mut self, start: Token) -> Expression {
        Expression::True(Box::new(True { location: start.location }))
    }

    fn nil_literal(&mut self, start: Token) -> Expression {
        Expression::Nil(Box::new(Nil { location: start.location }))
    }

    fn false_literal(&mut self, start: Token) -> Expression {
        Expression::False(Box::new(False { location: start.location }))
    }

    fn group_or_tuple(
        &mut self,
        start: Token,
    ) -> Result<Expression, ParseError> {
        let value_token = self.require()?;
        let value = self.expression(value_token)?;

        if self.peek().kind == TokenKind::Comma {
            let mut values = vec![value];

            self.next();

            loop {
                let token = self.require()?;

                if token.kind == TokenKind::ParenClose {
                    let location = SourceLocation::start_end(
                        &start.location,
                        &token.location,
                    );

                    return Ok(Expression::Tuple(Box::new(Tuple {
                        values,
                        location,
                    })));
                }

                values.push(self.expression(token)?);

                if self.peek().kind == TokenKind::Comma {
                    self.next();
                }
            }
        }

        let end = self.expect(TokenKind::ParenClose)?;
        let location =
            SourceLocation::start_end(&start.location, &end.location);

        Ok(Expression::Group(Box::new(Group { value, location })))
    }

    fn next_loop(&mut self, start: Token) -> Expression {
        Expression::Next(Box::new(Next { location: start.location }))
    }

    fn break_loop(&mut self, start: Token) -> Expression {
        Expression::Break(Box::new(Break { location: start.location }))
    }

    fn reference(&mut self, start: Token) -> Result<Expression, ParseError> {
        let value_token = self.require()?;
        let value = self.expression(value_token)?;
        let location =
            SourceLocation::start_end(&start.location, value.location());

        Ok(Expression::Ref(Box::new(Ref { value, location })))
    }

    fn recover_expression(
        &mut self,
        start: Token,
    ) -> Result<Expression, ParseError> {
        let body = self.expressions_with_optional_curly_braces()?;
        let location =
            SourceLocation::start_end(&start.location, &body.location);

        Ok(Expression::Recover(Box::new(Recover { body, location })))
    }

    fn mut_reference(
        &mut self,
        start: Token,
    ) -> Result<Expression, ParseError> {
        let value_token = self.require()?;
        let value = self.expression(value_token)?;
        let location =
            SourceLocation::start_end(&start.location, value.location());

        Ok(Expression::Mut(Box::new(Mut { value, location })))
    }

    fn throw_expression(
        &mut self,
        start: Token,
    ) -> Result<Expression, ParseError> {
        let value_token = self.require()?;
        let value = self.expression(value_token)?;
        let location =
            SourceLocation::start_end(&start.location, value.location());

        Ok(Expression::Throw(Box::new(Throw { value, location })))
    }

    fn return_expression(
        &mut self,
        start: Token,
    ) -> Result<Expression, ParseError> {
        let peeked = self.peek();
        let same_line =
            peeked.location.lines.start() == start.location.lines.start();

        let value = match peeked.kind {
            TokenKind::BracketOpen
            | TokenKind::Break
            | TokenKind::Constant
            | TokenKind::CurlyOpen
            | TokenKind::DoubleStringOpen
            | TokenKind::False
            | TokenKind::Field
            | TokenKind::Float
            | TokenKind::Fn
            | TokenKind::Identifier
            | TokenKind::If
            | TokenKind::Integer
            | TokenKind::Let
            | TokenKind::Loop
            | TokenKind::Match
            | TokenKind::Mut
            | TokenKind::Next
            | TokenKind::Nil
            | TokenKind::ParenOpen
            | TokenKind::Recover
            | TokenKind::Ref
            | TokenKind::Return
            | TokenKind::SelfObject
            | TokenKind::SingleStringOpen
            | TokenKind::Throw
            | TokenKind::True
            | TokenKind::Try
            | TokenKind::While
                if same_line =>
            {
                let token = self.next();

                Some(self.expression(token)?)
            }
            _ => None,
        };

        let end_loc = location!(value).unwrap_or(&start.location);
        let location = SourceLocation::start_end(&start.location, end_loc);

        Ok(Expression::Return(Box::new(Return { value, location })))
    }

    fn try_expression(
        &mut self,
        start: Token,
    ) -> Result<Expression, ParseError> {
        let expr_token = self.require()?;
        let expression = self.expression(expr_token)?;
        let end_loc = expression.location();
        let location = SourceLocation::start_end(&start.location, end_loc);

        Ok(Expression::Try(Box::new(Try { value: expression, location })))
    }

    fn if_expression(
        &mut self,
        start: Token,
    ) -> Result<Expression, ParseError> {
        let if_true = self.if_condition()?;
        let mut else_if = Vec::new();
        let mut else_body = None;

        while self.peek().kind == TokenKind::Else {
            self.next();

            if self.peek().kind == TokenKind::If {
                self.next();
                else_if.push(self.if_condition()?);
            } else {
                let token = self.expect(TokenKind::CurlyOpen)?;
                else_body = Some(self.expressions(token)?);

                break;
            }
        }

        let end_loc = location!(else_body)
            .or_else(|| location!(else_if.last()))
            .unwrap_or_else(|| if_true.location());
        let location = SourceLocation::start_end(&start.location, end_loc);

        Ok(Expression::If(Box::new(If {
            if_true,
            else_if,
            else_body,
            location,
        })))
    }

    fn match_expression(
        &mut self,
        start: Token,
    ) -> Result<Expression, ParseError> {
        let expression = self.expression_without_trailing_block()?;

        self.expect(TokenKind::CurlyOpen)?;

        let mut cases = Vec::new();

        while self.peek().kind != TokenKind::CurlyClose {
            let token = self.require()?;
            let node = match token.kind {
                TokenKind::Case => {
                    MatchExpression::Case(Box::new(self.match_case(token)?))
                }
                TokenKind::Comment => {
                    MatchExpression::Comment(self.comment(token))
                }
                _ => error!(
                    token.location,
                    "expected 'case' or a comment, found '{}' instead",
                    token.value
                ),
            };

            cases.push(node);

            if self.peek().kind == TokenKind::Comma {
                self.next();
            }
        }

        let close = self.expect(TokenKind::CurlyClose)?;
        let location =
            SourceLocation::start_end(&start.location, &close.location);

        Ok(Expression::Match(Box::new(Match {
            expression,
            expressions: cases,
            location,
        })))
    }

    fn match_case(&mut self, start: Token) -> Result<MatchCase, ParseError> {
        let pattern = self.pattern()?;
        let guard = self.optional_match_guard()?;

        self.expect(TokenKind::Arrow)?;

        let body = self.match_case_body()?;
        let location =
            SourceLocation::start_end(&start.location, body.location());

        Ok(MatchCase { pattern, guard, body, location })
    }

    fn patterns(&mut self) -> Result<Vec<Pattern>, ParseError> {
        let mut patterns = Vec::new();

        while let TokenKind::Identifier
        | TokenKind::Constant
        | TokenKind::Integer
        | TokenKind::DoubleStringOpen
        | TokenKind::SingleStringOpen
        | TokenKind::True
        | TokenKind::False
        | TokenKind::ParenOpen
        | TokenKind::Mut
        | TokenKind::CurlyOpen = self.peek().kind
        {
            patterns.push(self.pattern()?);

            if self.peek().kind == TokenKind::Comma {
                self.next();
            } else {
                break;
            }
        }

        // loop {
        //     match self.peek().kind {
        //         TokenKind::Identifier
        //         | TokenKind::Constant
        //         | TokenKind::Integer
        //         | TokenKind::DoubleStringOpen
        //         | TokenKind::SingleStringOpen
        //         | TokenKind::True
        //         | TokenKind::False
        //         | TokenKind::ParenOpen
        //         | TokenKind::Mut
        //         | TokenKind::CurlyOpen => patterns.push(self.pattern()?),
        //         _ => break,
        //     }
        //
        //     if self.peek().kind == TokenKind::Comma {
        //         self.next();
        //     } else {
        //         break;
        //     }
        // }

        Ok(patterns)
    }

    fn pattern(&mut self) -> Result<Pattern, ParseError> {
        let pat = self.pattern_without_or()?;

        if self.peek().kind == TokenKind::Or {
            let mut patterns = vec![pat];

            while self.peek().kind == TokenKind::Or {
                self.next();
                patterns.push(self.pattern_without_or()?);
            }

            let location = SourceLocation::start_end(
                patterns[0].location(),
                patterns.last().unwrap().location(),
            );

            Ok(Pattern::Or(Box::new(OrPattern { patterns, location })))
        } else {
            Ok(pat)
        }
    }

    fn pattern_without_or(&mut self) -> Result<Pattern, ParseError> {
        let token = self.require()?;
        let pattern = match token.kind {
            TokenKind::Identifier if token.value == "_" => {
                Pattern::Wildcard(Box::new(WildcardPattern {
                    location: token.location,
                }))
            }
            TokenKind::Identifier if self.peek().kind == TokenKind::Dot => {
                let source = Identifier::from(token);

                self.next();

                let name_token = self.expect(TokenKind::Constant)?;
                let location = SourceLocation::start_end(
                    &source.location,
                    &name_token.location,
                );

                Pattern::Constant(Box::new(Constant {
                    source: Some(source),
                    name: name_token.value,
                    location,
                }))
            }
            TokenKind::Identifier | TokenKind::Mut => {
                Pattern::Identifier(Box::new(self.identifier_pattern(token)?))
            }
            TokenKind::Constant if self.peek().kind == TokenKind::ParenOpen => {
                self.next();

                let name = Constant::from(token);
                let values = self.patterns()?;
                let close = self.expect(TokenKind::ParenClose)?;
                let location =
                    SourceLocation::start_end(&name.location, &close.location);

                Pattern::Variant(Box::new(VariantPattern {
                    name,
                    values,
                    location,
                }))
            }
            TokenKind::Constant => {
                Pattern::Constant(Box::new(Constant::from(token)))
            }
            TokenKind::Integer => Pattern::Int(Box::new(IntLiteral {
                value: token.value,
                location: token.location,
            })),
            TokenKind::DoubleStringOpen => {
                self.string_pattern(token, TokenKind::DoubleStringClose)?
            }
            TokenKind::SingleStringOpen => {
                self.string_pattern(token, TokenKind::SingleStringClose)?
            }
            TokenKind::True => {
                Pattern::True(Box::new(True { location: token.location }))
            }
            TokenKind::False => {
                Pattern::False(Box::new(False { location: token.location }))
            }
            TokenKind::ParenOpen => {
                let values = self.patterns()?;
                let close = self.expect(TokenKind::ParenClose)?;
                let location =
                    SourceLocation::start_end(&token.location, &close.location);

                Pattern::Tuple(Box::new(TuplePattern { values, location }))
            }
            TokenKind::CurlyOpen => {
                Pattern::Class(Box::new(self.class_pattern(token)?))
            }
            _ => {
                error!(
                    token.location,
                    "'{}' isn't a valid pattern", token.value
                );
            }
        };

        Ok(pattern)
    }

    fn string_pattern(
        &mut self,
        start: Token,
        close: TokenKind,
    ) -> Result<Pattern, ParseError> {
        let node = self.string_literal(start, close, false)?;

        Ok(Pattern::String(Box::new(node)))
    }

    fn identifier_pattern(
        &mut self,
        start: Token,
    ) -> Result<IdentifierPattern, ParseError> {
        if start.kind == TokenKind::Mut {
            let name = Identifier::from(self.expect(TokenKind::Identifier)?);
            let value_type = self.optional_type_annotation()?;
            let end_loc =
                location!(value_type).unwrap_or_else(|| name.location());
            let location = SourceLocation::start_end(&start.location, end_loc);

            return Ok(IdentifierPattern {
                name,
                value_type,
                mutable: true,
                location,
            });
        }

        if start.kind == TokenKind::Identifier {
            let name = Identifier::from(start);
            let value_type = self.optional_type_annotation()?;
            let end_loc =
                location!(value_type).unwrap_or_else(|| name.location());
            let location = SourceLocation::start_end(name.location(), end_loc);

            return Ok(IdentifierPattern {
                name,
                value_type,
                mutable: false,
                location,
            });
        }

        error!(
            start.location,
            "expected 'mut' or an identifier, found '{}' instead", start.value
        );
    }

    fn class_pattern(
        &mut self,
        start: Token,
    ) -> Result<ClassPattern, ParseError> {
        let mut values = Vec::new();

        while self.peek().kind != TokenKind::CurlyClose {
            let field = Field::from(self.expect(TokenKind::Field)?);

            self.expect(TokenKind::Assign)?;

            let pattern = self.pattern()?;
            let location =
                SourceLocation::start_end(&field.location, pattern.location());

            values.push(FieldPattern { field, pattern, location });

            if self.peek().kind == TokenKind::Comma {
                self.next();
            } else {
                break;
            }
        }

        let close = self.expect(TokenKind::CurlyClose)?;
        let location =
            SourceLocation::start_end(&start.location, &close.location);

        Ok(ClassPattern { values, location })
    }

    fn optional_match_guard(
        &mut self,
    ) -> Result<Option<Expression>, ParseError> {
        let result = if self.peek().kind == TokenKind::If {
            self.next();

            let token = self.require()?;

            Some(self.expression(token)?)
        } else {
            None
        };

        Ok(result)
    }

    fn match_case_body(&mut self) -> Result<Expressions, ParseError> {
        let start = self.require()?;

        let result = if start.kind == TokenKind::CurlyOpen {
            self.expressions(start)?
        } else {
            let expr = self.expression(start)?;
            let loc = expr.location().clone();

            Expressions { values: vec![expr], location: loc }
        };

        Ok(result)
    }

    fn loop_expression(
        &mut self,
        start: Token,
    ) -> Result<Expression, ParseError> {
        let body_token = self.expect(TokenKind::CurlyOpen)?;
        let body = self.expressions(body_token)?;
        let location =
            SourceLocation::start_end(&start.location, body.location());

        Ok(Expression::Loop(Box::new(Loop { body, location })))
    }

    fn while_expression(
        &mut self,
        start: Token,
    ) -> Result<Expression, ParseError> {
        let condition = self.expression_without_trailing_block()?;
        let body_token = self.expect(TokenKind::CurlyOpen)?;
        let body = self.expressions(body_token)?;
        let location =
            SourceLocation::start_end(&start.location, body.location());

        Ok(Expression::While(Box::new(While { condition, body, location })))
    }

    fn if_condition(&mut self) -> Result<IfCondition, ParseError> {
        let condition = self.expression_without_trailing_block()?;
        let token = self.expect(TokenKind::CurlyOpen)?;
        let body = self.expressions(token)?;
        let location =
            SourceLocation::start_end(condition.location(), body.location());

        Ok(IfCondition { condition, body, location })
    }

    fn comment(&mut self, start: Token) -> Box<Comment> {
        Box::new(Comment { value: start.value, location: start.location })
    }

    fn next(&mut self) -> Token {
        loop {
            let token =
                self.peeked.take().unwrap_or_else(|| self.lexer.next_token());

            match token.kind {
                TokenKind::Comment if self.comments => return token,
                TokenKind::Comment | TokenKind::Whitespace => {}
                _ => return token,
            }
        }
    }

    fn peek(&mut self) -> &Token {
        if self.peeked.is_none() {
            self.peeked = Some(self.next());
        }

        self.peeked.as_ref().unwrap()
    }

    fn require_valid_token(&self, token: &Token) -> Result<(), ParseError> {
        match token.kind {
            TokenKind::Invalid => {
                error!(
                    token.location.clone(),
                    "A '{}' is not allowed", token.value
                )
            }
            TokenKind::Null => {
                error!(
                    token.location.clone(),
                    "The end of the file is reached, but more input is expected"
                )
            }
            _ => Ok(()),
        }
    }

    fn require_token_kind(
        &self,
        token: &Token,
        kind: TokenKind,
    ) -> Result<(), ParseError> {
        self.require_valid_token(token)?;

        if token.kind != kind {
            error!(
                token.location.clone(),
                "expected {}, found '{}' instead",
                kind.description(),
                token.value.clone()
            );
        }

        Ok(())
    }

    fn require(&mut self) -> Result<Token, ParseError> {
        let token = self.next();

        self.require_valid_token(&token)?;
        Ok(token)
    }

    fn expect(&mut self, kind: TokenKind) -> Result<Token, ParseError> {
        let token = self.require()?;

        self.require_token_kind(&token, kind)?;
        Ok(token)
    }

    fn list<T, F>(
        &mut self,
        open: TokenKind,
        close: TokenKind,
        mut func: F,
    ) -> Result<(Vec<T>, SourceLocation), ParseError>
    where
        F: FnMut(&mut Self, Token) -> Result<T, ParseError>,
    {
        let mut values = Vec::new();
        let open_token = self.expect(open)?;

        loop {
            let token = self.require()?;

            if token.kind == close {
                return Ok((
                    values,
                    SourceLocation::start_end(
                        &open_token.location,
                        &token.location,
                    ),
                ));
            }

            values.push(func(self, token)?);

            if !values.is_empty() && self.peek().kind != close {
                self.expect(TokenKind::Comma)?;
            } else if self.peek().kind == TokenKind::Comma {
                self.next();
            }
        }
    }

    fn next_is_public(&mut self) -> bool {
        if self.peek().kind == TokenKind::Pub {
            self.next();
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use similar_asserts::assert_eq;
    use std::ops::RangeInclusive;

    pub(crate) fn cols(start: usize, stop: usize) -> SourceLocation {
        SourceLocation::new(1..=1, start..=stop)
    }

    fn location(
        line_range: RangeInclusive<usize>,
        column_range: RangeInclusive<usize>,
    ) -> SourceLocation {
        SourceLocation::new(line_range, column_range)
    }

    fn parser(input: &str) -> Parser {
        Parser::new(input.into(), "test.inko".into())
    }

    #[track_caller]
    fn parse(input: &str) -> Module {
        parser(input)
            .parse()
            .map_err(|e| format!("{} in {:?}", e.message, e.location))
            .unwrap()
    }

    #[track_caller]
    fn parse_with_comments(input: &str) -> Module {
        let mut parser = parser(input);

        parser.comments = true;
        parser
            .parse()
            .map_err(|e| format!("{} in {:?}", e.message, e.location))
            .unwrap()
    }

    fn top(mut ast: Module) -> TopLevelExpression {
        ast.expressions
            .pop()
            .expect("expected at least a single top-level expression")
    }

    #[track_caller]
    fn expr(input: &str) -> Expression {
        let mut parser = parser(input);
        let start = parser.require().unwrap();

        parser.expression(start).unwrap()
    }

    #[track_caller]
    fn expr_with_comments(input: &str) -> Expression {
        let mut parser = parser(input);

        parser.comments = true;

        let start = parser.require().unwrap();

        parser.expression(start).unwrap()
    }

    macro_rules! assert_error {
        ($input: expr, $location: expr) => {{
            let loc = $location;
            let ast = Parser::new($input.into(), "test.inko".into())
                .parse();

            if let Err(e) = ast {
                if e.location != loc {
                    panic!(
                        "expected a syntax error for {:?}, but found one for {:?}",
                        loc,
                        e.location
                    );
                }
            } else {
                panic!(
                    "expected a syntax error for {:?}, but no error was produced",
                    loc
                );
            }
        }};
    }

    macro_rules! assert_error_expr {
        ($input: expr, $location: expr) => {{
            let loc = $location;
            let mut parser = Parser::new($input.into(), "test.inko".into());
            let start = parser.require().unwrap();
            let result = parser.expression(start);

            if let Err(e) = result {
                if e.location != loc {
                    panic!(
                        "expected a syntax error for {:?}, but found one for {:?}",
                        loc,
                        e.location
                    );
                }
            } else {
                panic!(
                    "expected a syntax error for {:?}, but no error was produced",
                    loc
                );
            }
        }};
    }

    #[test]
    fn test_empty_module() {
        assert_eq!(
            parse(""),
            Module {
                expressions: Vec::new(),
                file: PathBuf::from("test.inko"),
                location: cols(1, 1)
            }
        );

        assert_eq!(
            parse("  "),
            Module {
                expressions: Vec::new(),
                file: PathBuf::from("test.inko"),
                location: cols(1, 2)
            }
        );

        assert_eq!(
            parse("\n  "),
            Module {
                expressions: Vec::new(),
                file: PathBuf::from("test.inko"),
                location: location(1..=2, 1..=2)
            }
        );
    }

    #[test]
    fn test_imports() {
        assert_eq!(
            top(parse("import foo")),
            TopLevelExpression::Import(Box::new(Import {
                path: ImportPath {
                    steps: vec![Identifier {
                        name: "foo".to_string(),
                        location: cols(8, 10)
                    }],
                    location: cols(8, 10)
                },
                symbols: None,
                tags: None,
                include: true,
                location: cols(1, 10)
            }))
        );

        assert_eq!(
            top(parse("import mut")),
            TopLevelExpression::Import(Box::new(Import {
                path: ImportPath {
                    steps: vec![Identifier {
                        name: "mut".to_string(),
                        location: cols(8, 10)
                    }],
                    location: cols(8, 10)
                },
                symbols: None,
                tags: None,
                include: true,
                location: cols(1, 10)
            }))
        );

        assert_eq!(
            top(parse("import foo ")),
            TopLevelExpression::Import(Box::new(Import {
                path: ImportPath {
                    steps: vec![Identifier {
                        name: "foo".to_string(),
                        location: cols(8, 10)
                    }],
                    location: cols(8, 10)
                },
                symbols: None,
                tags: None,
                include: true,
                location: cols(1, 10)
            }))
        );

        assert_eq!(
            top(parse("import foo.bar")),
            TopLevelExpression::Import(Box::new(Import {
                path: ImportPath {
                    steps: vec![
                        Identifier {
                            name: "foo".to_string(),
                            location: cols(8, 10)
                        },
                        Identifier {
                            name: "bar".to_string(),
                            location: cols(12, 14)
                        }
                    ],
                    location: cols(8, 14)
                },
                symbols: None,
                tags: None,
                include: true,
                location: cols(1, 14)
            }))
        );
    }

    #[test]
    fn test_conditional_import() {
        assert_eq!(
            top(parse("import foo if foo and bar")),
            TopLevelExpression::Import(Box::new(Import {
                path: ImportPath {
                    steps: vec![Identifier {
                        name: "foo".to_string(),
                        location: cols(8, 10)
                    }],
                    location: cols(8, 10)
                },
                symbols: None,
                tags: Some(BuildTags {
                    values: vec![
                        Identifier {
                            name: "foo".to_string(),
                            location: cols(15, 17)
                        },
                        Identifier {
                            name: "bar".to_string(),
                            location: cols(23, 25)
                        }
                    ],
                    location: cols(12, 25)
                }),
                include: true,
                location: cols(1, 25)
            }))
        );
    }

    #[test]
    fn test_extern_imports() {
        assert_eq!(
            top(parse("import extern 'foo'")),
            TopLevelExpression::ExternImport(Box::new(ExternImport {
                path: ExternImportPath {
                    path: "foo".to_string(),
                    location: cols(15, 19)
                },
                location: cols(1, 19)
            }))
        );

        assert_eq!(
            top(parse("import extern \"foo\"")),
            TopLevelExpression::ExternImport(Box::new(ExternImport {
                path: ExternImportPath {
                    path: "foo".to_string(),
                    location: cols(15, 19)
                },
                location: cols(1, 19)
            }))
        );

        assert_error!("import extern ''", cols(16, 16));
        assert_error!("import extern \"\"", cols(16, 16));
    }

    #[test]
    fn test_imports_with_symbols() {
        assert_eq!(
            top(parse("import foo.bar ()")),
            TopLevelExpression::Import(Box::new(Import {
                path: ImportPath {
                    steps: vec![
                        Identifier {
                            name: "foo".to_string(),
                            location: cols(8, 10)
                        },
                        Identifier {
                            name: "bar".to_string(),
                            location: cols(12, 14)
                        }
                    ],
                    location: cols(8, 14)
                },
                symbols: Some(ImportSymbols {
                    values: Vec::new(),
                    location: cols(16, 17)
                }),
                tags: None,
                include: true,
                location: cols(1, 17)
            }))
        );

        assert_eq!(
            top(parse("import foo (bar)")),
            TopLevelExpression::Import(Box::new(Import {
                path: ImportPath {
                    steps: vec![Identifier {
                        name: "foo".to_string(),
                        location: cols(8, 10)
                    }],
                    location: cols(8, 10)
                },
                symbols: Some(ImportSymbols {
                    values: vec![ImportSymbol {
                        name: "bar".to_string(),
                        alias: None,
                        location: cols(13, 15)
                    }],
                    location: cols(12, 16)
                }),
                tags: None,
                include: true,
                location: cols(1, 16)
            }))
        );

        assert_eq!(
            top(parse("import foo (bar, baz)")),
            TopLevelExpression::Import(Box::new(Import {
                path: ImportPath {
                    steps: vec![Identifier {
                        name: "foo".to_string(),
                        location: cols(8, 10)
                    }],
                    location: cols(8, 10)
                },
                symbols: Some(ImportSymbols {
                    values: vec![
                        ImportSymbol {
                            name: "bar".to_string(),
                            alias: None,
                            location: cols(13, 15)
                        },
                        ImportSymbol {
                            name: "baz".to_string(),
                            alias: None,
                            location: cols(18, 20)
                        }
                    ],
                    location: cols(12, 21)
                }),
                tags: None,
                include: true,
                location: cols(1, 21)
            }))
        );

        assert_eq!(
            top(parse("import foo (bar, baz,)")),
            TopLevelExpression::Import(Box::new(Import {
                path: ImportPath {
                    steps: vec![Identifier {
                        name: "foo".to_string(),
                        location: cols(8, 10)
                    }],
                    location: cols(8, 10)
                },
                symbols: Some(ImportSymbols {
                    values: vec![
                        ImportSymbol {
                            name: "bar".to_string(),
                            alias: None,
                            location: cols(13, 15)
                        },
                        ImportSymbol {
                            name: "baz".to_string(),
                            alias: None,
                            location: cols(18, 20)
                        }
                    ],
                    location: cols(12, 22)
                }),
                tags: None,
                include: true,
                location: cols(1, 22)
            }))
        );
    }

    #[test]
    fn test_imports_with_self() {
        assert_eq!(
            top(parse("import foo (self)")),
            TopLevelExpression::Import(Box::new(Import {
                path: ImportPath {
                    steps: vec![Identifier {
                        name: "foo".to_string(),
                        location: cols(8, 10)
                    }],
                    location: cols(8, 10)
                },
                symbols: Some(ImportSymbols {
                    values: vec![ImportSymbol {
                        name: "self".to_string(),
                        alias: None,
                        location: cols(13, 16)
                    }],
                    location: cols(12, 17)
                }),
                tags: None,
                include: true,
                location: cols(1, 17)
            }))
        );
    }

    #[test]
    fn test_imports_with_aliases() {
        assert_eq!(
            top(parse("import foo (bar as baz)")),
            TopLevelExpression::Import(Box::new(Import {
                path: ImportPath {
                    steps: vec![Identifier {
                        name: "foo".to_string(),
                        location: cols(8, 10)
                    }],
                    location: cols(8, 10)
                },
                symbols: Some(ImportSymbols {
                    values: vec![ImportSymbol {
                        name: "bar".to_string(),
                        alias: Some(ImportAlias {
                            name: "baz".to_string(),
                            location: cols(20, 22)
                        }),
                        location: cols(13, 15)
                    }],
                    location: cols(12, 23)
                }),
                tags: None,
                include: true,
                location: cols(1, 23)
            }))
        );

        assert_eq!(
            top(parse("import foo (Bar as Baz)")),
            TopLevelExpression::Import(Box::new(Import {
                path: ImportPath {
                    steps: vec![Identifier {
                        name: "foo".to_string(),
                        location: cols(8, 10)
                    }],
                    location: cols(8, 10)
                },
                symbols: Some(ImportSymbols {
                    values: vec![ImportSymbol {
                        name: "Bar".to_string(),
                        alias: Some(ImportAlias {
                            name: "Baz".to_string(),
                            location: cols(20, 22)
                        }),
                        location: cols(13, 15)
                    }],
                    location: cols(12, 23)
                }),
                tags: None,
                include: true,
                location: cols(1, 23)
            }))
        );

        assert_eq!(
            top(parse("import foo (self as foo)")),
            TopLevelExpression::Import(Box::new(Import {
                path: ImportPath {
                    steps: vec![Identifier {
                        name: "foo".to_string(),
                        location: cols(8, 10)
                    }],
                    location: cols(8, 10)
                },
                symbols: Some(ImportSymbols {
                    values: vec![ImportSymbol {
                        name: "self".to_string(),
                        alias: Some(ImportAlias {
                            name: "foo".to_string(),
                            location: cols(21, 23)
                        }),
                        location: cols(13, 16)
                    }],
                    location: cols(12, 24)
                }),
                tags: None,
                include: true,
                location: cols(1, 24)
            }))
        );

        assert_eq!(
            top(parse("import foo (self as _)")),
            TopLevelExpression::Import(Box::new(Import {
                path: ImportPath {
                    steps: vec![Identifier {
                        name: "foo".to_string(),
                        location: cols(8, 10)
                    }],
                    location: cols(8, 10)
                },
                symbols: Some(ImportSymbols {
                    values: vec![ImportSymbol {
                        name: "self".to_string(),
                        alias: Some(ImportAlias {
                            name: "_".to_string(),
                            location: cols(21, 21)
                        }),
                        location: cols(13, 16)
                    }],
                    location: cols(12, 22)
                }),
                tags: None,
                include: true,
                location: cols(1, 22)
            }))
        );
    }

    #[test]
    fn test_invalid_imports() {
        assert_error!("import foo (bar as Baz)", cols(20, 22));
        assert_error!("import foo (bar as *)", cols(20, 20));
        assert_error!("import foo (self as Baz)", cols(21, 23));
        assert_error!("import foo (Bar as baz)", cols(20, 22));
        assert_error!("import foo (,)", cols(13, 13));
        assert_error!("import foo.", cols(11, 11));
        assert_error!("import foo (", cols(12, 12));
        assert_error!("import foo )", cols(12, 12));
    }

    #[test]
    fn test_constant_without_type_signature() {
        assert_eq!(
            top(parse("let A = 10")),
            TopLevelExpression::DefineConstant(Box::new(DefineConstant {
                public: false,
                name: Constant {
                    source: None,
                    name: "A".to_string(),
                    location: cols(5, 5)
                },
                value: Expression::Int(Box::new(IntLiteral {
                    value: "10".to_string(),
                    location: cols(9, 10)
                })),
                location: cols(1, 10)
            }))
        );

        assert_eq!(
            top(parse("let pub A = 10")),
            TopLevelExpression::DefineConstant(Box::new(DefineConstant {
                public: true,
                name: Constant {
                    source: None,
                    name: "A".to_string(),
                    location: cols(9, 9)
                },
                value: Expression::Int(Box::new(IntLiteral {
                    value: "10".to_string(),
                    location: cols(13, 14)
                })),
                location: cols(1, 14)
            }))
        );
    }

    #[test]
    fn test_constant_array() {
        assert_eq!(
            top(parse("let A = [10]")),
            TopLevelExpression::DefineConstant(Box::new(DefineConstant {
                public: false,
                name: Constant {
                    source: None,
                    name: "A".to_string(),
                    location: cols(5, 5)
                },
                value: Expression::Array(Box::new(Array {
                    values: vec![Expression::Int(Box::new(IntLiteral {
                        value: "10".to_string(),
                        location: cols(10, 11)
                    }))],
                    location: cols(9, 12)
                })),
                location: cols(1, 12)
            }))
        );

        assert_eq!(
            top(parse_with_comments("let A = [10\n#a\n]")),
            TopLevelExpression::DefineConstant(Box::new(DefineConstant {
                public: false,
                name: Constant {
                    source: None,
                    name: "A".to_string(),
                    location: cols(5, 5)
                },
                value: Expression::Array(Box::new(Array {
                    values: vec![
                        Expression::Int(Box::new(IntLiteral {
                            value: "10".to_string(),
                            location: cols(10, 11)
                        })),
                        Expression::Comment(Box::new(Comment {
                            value: "a".to_string(),
                            location: location(2..=2, 1..=2)
                        }))
                    ],
                    location: location(1..=3, 9..=1)
                })),
                location: location(1..=3, 1..=1)
            }))
        );

        assert_eq!(
            top(parse("let A = [true, false]")),
            TopLevelExpression::DefineConstant(Box::new(DefineConstant {
                public: false,
                name: Constant {
                    source: None,
                    name: "A".to_string(),
                    location: cols(5, 5)
                },
                value: Expression::Array(Box::new(Array {
                    values: vec![
                        Expression::True(Box::new(True {
                            location: cols(10, 13)
                        })),
                        Expression::False(Box::new(False {
                            location: cols(16, 20)
                        }))
                    ],
                    location: cols(9, 21)
                })),
                location: cols(1, 21)
            }))
        );
    }

    #[test]
    fn test_constant_with_namespaced_constant_value() {
        assert_eq!(
            top(parse("let A = a.B")),
            TopLevelExpression::DefineConstant(Box::new(DefineConstant {
                public: false,
                name: Constant {
                    source: None,
                    name: "A".to_string(),
                    location: cols(5, 5)
                },
                value: Expression::Constant(Box::new(Constant {
                    source: Some(Identifier {
                        name: "a".to_string(),
                        location: cols(9, 9)
                    }),
                    name: "B".to_string(),
                    location: cols(9, 11)
                })),
                location: cols(1, 11)
            }))
        );
    }

    #[test]
    fn test_invalid_constants() {
        assert_error!("let A = B.new", cols(10, 10));
        assert_error!("let A = B { }", cols(11, 11));
        assert_error!("let A = (B.new)", cols(11, 11));
    }

    #[test]
    fn test_constant_with_binary_operation() {
        assert_eq!(
            top(parse("let A = 10 + 5")),
            TopLevelExpression::DefineConstant(Box::new(DefineConstant {
                public: false,
                name: Constant {
                    source: None,
                    name: "A".to_string(),
                    location: cols(5, 5)
                },
                value: Expression::Binary(Box::new(Binary {
                    operator: Operator {
                        kind: OperatorKind::Add,
                        location: cols(12, 12)
                    },
                    left: Expression::Int(Box::new(IntLiteral {
                        value: "10".to_string(),
                        location: cols(9, 10)
                    })),
                    right: Expression::Int(Box::new(IntLiteral {
                        value: "5".to_string(),
                        location: cols(14, 14)
                    })),
                    location: cols(9, 14)
                })),
                location: cols(1, 14)
            }))
        );
    }

    #[test]
    fn test_optional_type_annotation_without_type() {
        let result = parser(" ").optional_type_annotation();

        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[test]
    fn test_optional_type_annotation_with_type() {
        let result = parser(": A").optional_type_annotation();

        assert_eq!(
            result.unwrap().unwrap(),
            Type::Named(Box::new(TypeName {
                name: Constant {
                    source: None,
                    name: "A".to_string(),
                    location: cols(3, 3),
                },
                arguments: None,
                location: cols(3, 3)
            }))
        );
    }

    #[test]
    fn test_type_reference_with_named_type() {
        let mut parser = parser("A");
        let start = parser.require().unwrap();

        assert_eq!(
            parser.type_reference(start).unwrap(),
            Type::Named(Box::new(TypeName {
                name: Constant {
                    source: None,
                    name: "A".to_string(),
                    location: cols(1, 1),
                },
                arguments: None,
                location: cols(1, 1)
            }))
        );
    }

    #[test]
    fn test_type_reference_with_named_type_with_source() {
        let mut parser = parser("a.A");
        let start = parser.require().unwrap();

        assert_eq!(
            parser.type_reference(start).unwrap(),
            Type::Named(Box::new(TypeName {
                name: Constant {
                    source: Some(Identifier {
                        name: "a".to_string(),
                        location: cols(1, 1)
                    }),
                    name: "A".to_string(),
                    location: cols(3, 3),
                },
                arguments: None,
                location: cols(1, 3)
            }))
        );
    }

    #[test]
    fn test_type_reference_with_reference_type() {
        let mut parser = parser("ref A");
        let start = parser.require().unwrap();

        assert_eq!(
            parser.type_reference(start).unwrap(),
            Type::Ref(Box::new(ReferenceType {
                type_reference: ReferrableType::Named(Box::new(TypeName {
                    name: Constant {
                        source: None,
                        name: "A".to_string(),
                        location: cols(5, 5),
                    },
                    arguments: None,
                    location: cols(5, 5)
                })),
                location: cols(1, 5)
            }))
        );
    }

    #[test]
    fn test_type_reference_with_namespaced_reference_type() {
        let mut parser = parser("ref a.A");
        let start = parser.require().unwrap();

        assert_eq!(
            parser.type_reference(start).unwrap(),
            Type::Ref(Box::new(ReferenceType {
                type_reference: ReferrableType::Named(Box::new(TypeName {
                    name: Constant {
                        source: Some(Identifier {
                            name: "a".to_string(),
                            location: cols(5, 5)
                        }),
                        name: "A".to_string(),
                        location: cols(7, 7),
                    },
                    arguments: None,
                    location: cols(5, 7)
                })),
                location: cols(1, 7)
            }))
        );
    }

    #[test]
    fn test_type_reference_with_mut_reference_type() {
        let mut parser = parser("mut A");
        let start = parser.require().unwrap();

        assert_eq!(
            parser.type_reference(start).unwrap(),
            Type::Mut(Box::new(ReferenceType {
                type_reference: ReferrableType::Named(Box::new(TypeName {
                    name: Constant {
                        source: None,
                        name: "A".to_string(),
                        location: cols(5, 5),
                    },
                    arguments: None,
                    location: cols(5, 5)
                })),
                location: cols(1, 5)
            }))
        );
    }

    #[test]
    fn test_type_reference_with_owned_reference_type() {
        let mut parser = parser("move A");
        let start = parser.require().unwrap();

        assert_eq!(
            parser.type_reference(start).unwrap(),
            Type::Owned(Box::new(ReferenceType {
                type_reference: ReferrableType::Named(Box::new(TypeName {
                    name: Constant {
                        source: None,
                        name: "A".to_string(),
                        location: cols(6, 6),
                    },
                    arguments: None,
                    location: cols(6, 6)
                })),
                location: cols(1, 6)
            }))
        );
    }

    #[test]
    fn test_type_reference_with_double_reference() {
        let mut parser = parser("ref ref A");
        let start = parser.require().unwrap();

        assert!(parser.type_reference(start).is_err());
    }

    #[test]
    fn test_type_reference_with_simple_closure_type() {
        let mut parser = parser("fn");
        let start = parser.require().unwrap();

        assert_eq!(
            parser.type_reference(start).unwrap(),
            Type::Closure(Box::new(ClosureType {
                arguments: None,
                return_type: None,
                location: cols(1, 2)
            }))
        );
    }

    #[test]
    fn test_type_reference_with_closure_type_with_arguments() {
        let mut parser = parser("fn (T)");
        let start = parser.require().unwrap();

        assert_eq!(
            parser.type_reference(start).unwrap(),
            Type::Closure(Box::new(ClosureType {
                arguments: Some(Types {
                    values: vec![Type::Named(Box::new(TypeName {
                        name: Constant {
                            source: None,
                            name: "T".to_string(),
                            location: cols(5, 5),
                        },
                        arguments: None,
                        location: cols(5, 5)
                    }))],
                    location: cols(4, 6)
                }),
                return_type: None,
                location: cols(1, 6)
            }))
        );
    }

    #[test]
    fn test_type_reference_with_closure_type_with_return_type() {
        let mut parser = parser("fn -> T");
        let start = parser.require().unwrap();

        assert_eq!(
            parser.type_reference(start).unwrap(),
            Type::Closure(Box::new(ClosureType {
                arguments: None,
                return_type: Some(Type::Named(Box::new(TypeName {
                    name: Constant {
                        source: None,
                        name: "T".to_string(),
                        location: cols(7, 7),
                    },
                    arguments: None,
                    location: cols(7, 7)
                }))),
                location: cols(1, 7)
            }))
        );
    }

    #[test]
    fn test_type_reference_with_full_closure_type() {
        let mut parser = parser("fn (B) -> D");
        let start = parser.require().unwrap();

        assert_eq!(
            parser.type_reference(start).unwrap(),
            Type::Closure(Box::new(ClosureType {
                arguments: Some(Types {
                    values: vec![Type::Named(Box::new(TypeName {
                        name: Constant {
                            source: None,
                            name: "B".to_string(),
                            location: cols(5, 5),
                        },
                        arguments: None,
                        location: cols(5, 5)
                    }))],
                    location: cols(4, 6)
                }),
                return_type: Some(Type::Named(Box::new(TypeName {
                    name: Constant {
                        source: None,
                        name: "D".to_string(),
                        location: cols(11, 11),
                    },
                    arguments: None,
                    location: cols(11, 11)
                }))),
                location: cols(1, 11)
            }))
        );
    }

    #[test]
    fn test_type_reference_with_tuple_type() {
        let mut parser = parser("(A, B)");
        let start = parser.require().unwrap();

        assert_eq!(
            parser.type_reference(start).unwrap(),
            Type::Tuple(Box::new(TupleType {
                values: vec![
                    Type::Named(Box::new(TypeName {
                        name: Constant {
                            source: None,
                            name: "A".to_string(),
                            location: cols(2, 2)
                        },
                        arguments: None,
                        location: cols(2, 2)
                    })),
                    Type::Named(Box::new(TypeName {
                        name: Constant {
                            source: None,
                            name: "B".to_string(),
                            location: cols(5, 5)
                        },
                        arguments: None,
                        location: cols(5, 5)
                    })),
                ],
                location: cols(1, 6)
            }))
        );
    }

    #[test]
    fn test_type_reference_with_reference_tuple_type() {
        let mut parser = parser("ref (A, B)");
        let start = parser.require().unwrap();

        assert_eq!(
            parser.type_reference(start).unwrap(),
            Type::Ref(Box::new(ReferenceType {
                type_reference: ReferrableType::Tuple(Box::new(TupleType {
                    values: vec![
                        Type::Named(Box::new(TypeName {
                            name: Constant {
                                source: None,
                                name: "A".to_string(),
                                location: cols(6, 6)
                            },
                            arguments: None,
                            location: cols(6, 6)
                        })),
                        Type::Named(Box::new(TypeName {
                            name: Constant {
                                source: None,
                                name: "B".to_string(),
                                location: cols(9, 9)
                            },
                            arguments: None,
                            location: cols(9, 9)
                        })),
                    ],
                    location: cols(5, 10)
                })),
                location: cols(1, 10)
            }))
        );
    }

    #[test]
    fn test_type_reference_with_invalid_tuple_type() {
        let mut parser = parser("()");
        let start = parser.require().unwrap();
        let node = parser.type_reference(start);

        assert!(node.is_err());
    }

    #[test]
    fn test_methods() {
        assert_eq!(
            top(parse("fn foo {}")),
            TopLevelExpression::DefineMethod(Box::new(DefineMethod {
                public: false,
                operator: false,
                kind: MethodKind::Instance,
                name: Identifier {
                    name: "foo".to_string(),
                    location: cols(4, 6)
                },
                type_parameters: None,
                arguments: None,
                return_type: None,
                body: Some(Expressions {
                    values: Vec::new(),
                    location: cols(8, 9)
                }),
                location: cols(1, 9)
            }))
        );

        assert_eq!(
            top(parse("fn FOO {}")),
            TopLevelExpression::DefineMethod(Box::new(DefineMethod {
                public: false,
                operator: false,
                kind: MethodKind::Instance,
                name: Identifier {
                    name: "FOO".to_string(),
                    location: cols(4, 6)
                },
                type_parameters: None,
                arguments: None,
                return_type: None,
                body: Some(Expressions {
                    values: Vec::new(),
                    location: cols(8, 9)
                }),
                location: cols(1, 9)
            }))
        );

        assert_eq!(
            top(parse("fn pub foo {}")),
            TopLevelExpression::DefineMethod(Box::new(DefineMethod {
                public: true,
                operator: false,
                kind: MethodKind::Instance,
                name: Identifier {
                    name: "foo".to_string(),
                    location: cols(8, 10)
                },
                type_parameters: None,
                arguments: None,
                return_type: None,
                body: Some(Expressions {
                    values: Vec::new(),
                    location: cols(12, 13)
                }),
                location: cols(1, 13)
            }))
        );

        assert_eq!(
            top(parse("fn 123 {}")),
            TopLevelExpression::DefineMethod(Box::new(DefineMethod {
                public: false,
                operator: false,
                kind: MethodKind::Instance,
                name: Identifier {
                    name: "123".to_string(),
                    location: cols(4, 6)
                },
                type_parameters: None,
                arguments: None,
                return_type: None,
                body: Some(Expressions {
                    values: Vec::new(),
                    location: cols(8, 9)
                }),
                location: cols(1, 9)
            }))
        );

        assert_eq!(
            top(parse("fn ab= {}")),
            TopLevelExpression::DefineMethod(Box::new(DefineMethod {
                public: false,
                operator: false,
                kind: MethodKind::Instance,
                name: Identifier {
                    name: "ab=".to_string(),
                    location: cols(4, 6)
                },
                type_parameters: None,
                arguments: None,
                return_type: None,
                body: Some(Expressions {
                    values: Vec::new(),
                    location: cols(8, 9)
                }),
                location: cols(1, 9)
            }))
        );

        assert_eq!(
            top(parse("fn 12= {}")),
            TopLevelExpression::DefineMethod(Box::new(DefineMethod {
                public: false,
                operator: false,
                kind: MethodKind::Instance,
                name: Identifier {
                    name: "12=".to_string(),
                    location: cols(4, 6)
                },
                type_parameters: None,
                arguments: None,
                return_type: None,
                body: Some(Expressions {
                    values: Vec::new(),
                    location: cols(8, 9)
                }),
                location: cols(1, 9)
            }))
        );

        assert_eq!(
            top(parse("fn let {}")),
            TopLevelExpression::DefineMethod(Box::new(DefineMethod {
                public: false,
                operator: false,
                kind: MethodKind::Instance,
                name: Identifier {
                    name: "let".to_string(),
                    location: cols(4, 6)
                },
                type_parameters: None,
                arguments: None,
                return_type: None,
                body: Some(Expressions {
                    values: Vec::new(),
                    location: cols(8, 9)
                }),
                location: cols(1, 9)
            }))
        );
    }

    #[test]
    fn test_methods_with_type_parameters() {
        assert_eq!(
            top(parse("fn foo [T] {}")),
            TopLevelExpression::DefineMethod(Box::new(DefineMethod {
                public: false,
                operator: false,
                kind: MethodKind::Instance,
                name: Identifier {
                    name: "foo".to_string(),
                    location: cols(4, 6)
                },
                type_parameters: Some(TypeParameters {
                    values: vec![TypeParameter {
                        name: Constant {
                            source: None,
                            name: "T".to_string(),
                            location: cols(9, 9)
                        },
                        requirements: None,
                        location: cols(9, 9)
                    }],
                    location: cols(8, 10)
                }),
                arguments: None,
                return_type: None,
                body: Some(Expressions {
                    values: Vec::new(),
                    location: cols(12, 13)
                }),
                location: cols(1, 13)
            }))
        );

        assert_eq!(
            top(parse("fn foo [T: A + B] {}")),
            TopLevelExpression::DefineMethod(Box::new(DefineMethod {
                public: false,
                operator: false,
                kind: MethodKind::Instance,
                name: Identifier {
                    name: "foo".to_string(),
                    location: cols(4, 6)
                },
                type_parameters: Some(TypeParameters {
                    values: vec![TypeParameter {
                        name: Constant {
                            source: None,
                            name: "T".to_string(),
                            location: cols(9, 9)
                        },
                        requirements: Some(Requirements {
                            values: vec![
                                Requirement::Trait(TypeName {
                                    name: Constant {
                                        source: None,
                                        name: "A".to_string(),
                                        location: cols(12, 12),
                                    },
                                    arguments: None,
                                    location: cols(12, 12)
                                }),
                                Requirement::Trait(TypeName {
                                    name: Constant {
                                        source: None,
                                        name: "B".to_string(),
                                        location: cols(16, 16),
                                    },
                                    arguments: None,
                                    location: cols(16, 16)
                                })
                            ],
                            location: cols(12, 16)
                        }),
                        location: cols(9, 16)
                    }],
                    location: cols(8, 17)
                }),
                arguments: None,
                return_type: None,
                body: Some(Expressions {
                    values: Vec::new(),
                    location: cols(19, 20)
                }),
                location: cols(1, 20)
            }))
        );
    }

    #[test]
    fn test_methods_with_arguments() {
        assert_eq!(
            top(parse("fn foo (a: A, b: B) {}")),
            TopLevelExpression::DefineMethod(Box::new(DefineMethod {
                public: false,
                operator: false,
                kind: MethodKind::Instance,
                name: Identifier {
                    name: "foo".to_string(),
                    location: cols(4, 6)
                },
                type_parameters: None,
                arguments: Some(MethodArguments {
                    values: vec![
                        MethodArgument {
                            name: Identifier {
                                name: "a".to_string(),
                                location: cols(9, 9)
                            },
                            value_type: Type::Named(Box::new(TypeName {
                                name: Constant {
                                    source: None,
                                    name: "A".to_string(),
                                    location: cols(12, 12),
                                },
                                arguments: None,
                                location: cols(12, 12)
                            })),
                            location: cols(9, 12),
                        },
                        MethodArgument {
                            name: Identifier {
                                name: "b".to_string(),
                                location: cols(15, 15)
                            },
                            value_type: Type::Named(Box::new(TypeName {
                                name: Constant {
                                    source: None,
                                    name: "B".to_string(),
                                    location: cols(18, 18),
                                },
                                arguments: None,
                                location: cols(18, 18)
                            })),
                            location: cols(15, 18),
                        }
                    ],
                    variadic: false,
                    location: cols(8, 19)
                }),
                return_type: None,
                body: Some(Expressions {
                    values: Vec::new(),
                    location: cols(21, 22)
                }),
                location: cols(1, 22)
            }))
        );
    }

    #[test]
    fn test_method_with_return_type() {
        assert_eq!(
            top(parse("fn foo -> A {}")),
            TopLevelExpression::DefineMethod(Box::new(DefineMethod {
                public: false,
                operator: false,
                kind: MethodKind::Instance,
                name: Identifier {
                    name: "foo".to_string(),
                    location: cols(4, 6)
                },
                type_parameters: None,
                arguments: None,
                return_type: Some(Type::Named(Box::new(TypeName {
                    name: Constant {
                        source: None,
                        name: "A".to_string(),
                        location: cols(11, 11),
                    },
                    arguments: None,
                    location: cols(11, 11)
                }))),
                body: Some(Expressions {
                    values: Vec::new(),
                    location: cols(13, 14)
                }),
                location: cols(1, 14)
            }))
        );
    }

    #[test]
    fn test_method_with_body() {
        assert_eq!(
            top(parse("fn foo { 10 }")),
            TopLevelExpression::DefineMethod(Box::new(DefineMethod {
                public: false,
                operator: false,
                kind: MethodKind::Instance,
                name: Identifier {
                    name: "foo".to_string(),
                    location: cols(4, 6)
                },
                type_parameters: None,
                arguments: None,
                return_type: None,
                body: Some(Expressions {
                    values: vec![Expression::Int(Box::new(IntLiteral {
                        value: "10".to_string(),
                        location: cols(10, 11)
                    }))],
                    location: cols(8, 13)
                }),
                location: cols(1, 13),
            }))
        );
    }

    #[test]
    fn test_extern_method() {
        assert_eq!(
            top(parse("fn extern foo")),
            TopLevelExpression::DefineMethod(Box::new(DefineMethod {
                public: false,
                operator: false,
                kind: MethodKind::Extern,
                name: Identifier {
                    name: "foo".to_string(),
                    location: cols(11, 13)
                },
                type_parameters: None,
                arguments: None,
                return_type: None,
                body: None,
                location: cols(1, 13),
            }))
        );

        assert_eq!(
            top(parse("fn extern foo {}")),
            TopLevelExpression::DefineMethod(Box::new(DefineMethod {
                public: false,
                operator: false,
                kind: MethodKind::Extern,
                name: Identifier {
                    name: "foo".to_string(),
                    location: cols(11, 13)
                },
                type_parameters: None,
                arguments: None,
                return_type: None,
                body: Some(Expressions {
                    values: Vec::new(),
                    location: cols(15, 16)
                }),
                location: cols(1, 16),
            }))
        );

        assert_eq!(
            top(parse("fn extern foo(...)")),
            TopLevelExpression::DefineMethod(Box::new(DefineMethod {
                public: false,
                operator: false,
                kind: MethodKind::Extern,
                name: Identifier {
                    name: "foo".to_string(),
                    location: cols(11, 13)
                },
                type_parameters: None,
                arguments: Some(MethodArguments {
                    values: Vec::new(),
                    variadic: true,
                    location: cols(14, 18)
                }),
                return_type: None,
                body: None,
                location: cols(1, 18),
            }))
        );

        assert_eq!(
            top(parse("fn extern foo(...,)")),
            TopLevelExpression::DefineMethod(Box::new(DefineMethod {
                public: false,
                operator: false,
                kind: MethodKind::Extern,
                name: Identifier {
                    name: "foo".to_string(),
                    location: cols(11, 13)
                },
                type_parameters: None,
                arguments: Some(MethodArguments {
                    values: Vec::new(),
                    variadic: true,
                    location: cols(14, 19)
                }),
                return_type: None,
                body: None,
                location: cols(1, 19),
            }))
        );
    }

    #[test]
    fn test_invalid_methods() {
        assert_error!("fn foo [ {}", cols(10, 10));
        assert_error!("fn foo [A: ] {}", cols(12, 12));
        assert_error!("fn foo (A: ) {}", cols(9, 9));
        assert_error!("fn foo (a: ) {}", cols(12, 12));
        assert_error!("fn foo -> {}", cols(11, 11));
        assert_error!("fn foo {", cols(8, 8));
        assert_error!("fn foo", cols(6, 6));
        assert_error!("fn extern foo[T](arg: T)", cols(14, 14));
        assert_error!("fn extern foo(...) {}", cols(20, 20));
    }

    #[test]
    fn test_empty_class() {
        assert_eq!(
            top(parse("class A {}")),
            TopLevelExpression::DefineClass(Box::new(DefineClass {
                public: false,
                name: Constant {
                    source: None,
                    name: "A".to_string(),
                    location: cols(7, 7)
                },
                kind: ClassKind::Regular,
                type_parameters: None,
                body: ClassExpressions {
                    values: Vec::new(),
                    location: cols(9, 10)
                },
                location: cols(1, 10)
            }))
        );

        assert_eq!(
            top(parse("class pub A {}")),
            TopLevelExpression::DefineClass(Box::new(DefineClass {
                public: true,
                name: Constant {
                    source: None,
                    name: "A".to_string(),
                    location: cols(11, 11)
                },
                kind: ClassKind::Regular,
                type_parameters: None,
                body: ClassExpressions {
                    values: Vec::new(),
                    location: cols(13, 14)
                },
                location: cols(1, 14)
            }))
        );
    }

    #[test]
    fn test_extern_class() {
        assert_eq!(
            top(parse("class extern A {}")),
            TopLevelExpression::DefineClass(Box::new(DefineClass {
                public: false,
                name: Constant {
                    source: None,
                    name: "A".to_string(),
                    location: cols(14, 14)
                },
                kind: ClassKind::Extern,
                type_parameters: None,
                body: ClassExpressions {
                    values: Vec::new(),
                    location: cols(16, 17)
                },
                location: cols(1, 17)
            }))
        );
    }

    #[test]
    fn test_class_literal() {
        assert_eq!(
            expr("A { @a = 10 }"),
            Expression::ClassLiteral(Box::new(ClassLiteral {
                class_name: Constant {
                    source: None,
                    name: "A".to_string(),
                    location: cols(1, 1)
                },
                fields: vec![AssignInstanceLiteralField {
                    field: Field {
                        name: "a".to_string(),
                        location: cols(5, 6)
                    },
                    value: Expression::Int(Box::new(IntLiteral {
                        value: "10".to_string(),
                        location: cols(10, 11)
                    })),
                    location: cols(5, 11)
                }],
                location: cols(1, 13)
            }))
        );

        assert_eq!(
            expr("A\n{ @a = 10 }"),
            Expression::Constant(Box::new(Constant {
                source: None,
                name: "A".to_string(),
                location: cols(1, 1)
            }))
        );

        assert_eq!(
            expr("A\n{ @a = 10, }"),
            Expression::Constant(Box::new(Constant {
                source: None,
                name: "A".to_string(),
                location: cols(1, 1)
            }))
        );

        assert_error_expr!("A { @a = 10 @b = 20 }", cols(13, 14));
    }

    #[test]
    fn test_async_class() {
        assert_eq!(
            top(parse("class async A {}")),
            TopLevelExpression::DefineClass(Box::new(DefineClass {
                public: false,
                name: Constant {
                    source: None,
                    name: "A".to_string(),
                    location: cols(13, 13)
                },
                kind: ClassKind::Async,
                type_parameters: None,
                body: ClassExpressions {
                    values: Vec::new(),
                    location: cols(15, 16)
                },
                location: cols(1, 16)
            }))
        );
    }

    #[test]
    fn test_class_with_async_method() {
        assert_eq!(
            top(parse("class A { fn async foo {} }")),
            TopLevelExpression::DefineClass(Box::new(DefineClass {
                public: false,
                name: Constant {
                    source: None,
                    name: "A".to_string(),
                    location: cols(7, 7)
                },
                kind: ClassKind::Regular,
                type_parameters: None,
                body: ClassExpressions {
                    values: vec![ClassExpression::DefineMethod(Box::new(
                        DefineMethod {
                            public: false,
                            operator: false,
                            kind: MethodKind::Async,
                            name: Identifier {
                                name: "foo".to_string(),
                                location: cols(20, 22)
                            },
                            type_parameters: None,
                            arguments: None,
                            return_type: None,
                            body: Some(Expressions {
                                values: Vec::new(),
                                location: cols(24, 25)
                            }),
                            location: cols(11, 25)
                        }
                    ))],
                    location: cols(9, 27)
                },
                location: cols(1, 27)
            }))
        );

        assert_eq!(
            top(parse("class A { fn async mut foo {} }")),
            TopLevelExpression::DefineClass(Box::new(DefineClass {
                public: false,
                name: Constant {
                    source: None,
                    name: "A".to_string(),
                    location: cols(7, 7)
                },
                kind: ClassKind::Regular,
                type_parameters: None,
                body: ClassExpressions {
                    values: vec![ClassExpression::DefineMethod(Box::new(
                        DefineMethod {
                            public: false,
                            operator: false,
                            kind: MethodKind::AsyncMutable,
                            name: Identifier {
                                name: "foo".to_string(),
                                location: cols(24, 26)
                            },
                            type_parameters: None,
                            arguments: None,
                            return_type: None,
                            body: Some(Expressions {
                                values: Vec::new(),
                                location: cols(28, 29)
                            }),
                            location: cols(11, 29)
                        }
                    ))],
                    location: cols(9, 31)
                },
                location: cols(1, 31)
            }))
        );
    }

    #[test]
    fn test_class_with_type_parameters() {
        assert_eq!(
            top(parse("class A[B: X, C] {}")),
            TopLevelExpression::DefineClass(Box::new(DefineClass {
                public: false,
                name: Constant {
                    source: None,
                    name: "A".to_string(),
                    location: cols(7, 7)
                },
                kind: ClassKind::Regular,
                type_parameters: Some(TypeParameters {
                    values: vec![
                        TypeParameter {
                            name: Constant {
                                source: None,
                                name: "B".to_string(),
                                location: cols(9, 9)
                            },
                            requirements: Some(Requirements {
                                values: vec![Requirement::Trait(TypeName {
                                    name: Constant {
                                        source: None,
                                        name: "X".to_string(),
                                        location: cols(12, 12),
                                    },
                                    arguments: None,
                                    location: cols(12, 12)
                                })],
                                location: cols(12, 12)
                            }),
                            location: cols(9, 12)
                        },
                        TypeParameter {
                            name: Constant {
                                source: None,
                                name: "C".to_string(),
                                location: cols(15, 15)
                            },
                            requirements: None,
                            location: cols(15, 15)
                        }
                    ],
                    location: cols(8, 16)
                }),
                body: ClassExpressions {
                    values: Vec::new(),
                    location: cols(18, 19)
                },
                location: cols(1, 19)
            }))
        );

        assert_eq!(
            top(parse("class A[B: a.X] {}")),
            TopLevelExpression::DefineClass(Box::new(DefineClass {
                public: false,
                name: Constant {
                    source: None,
                    name: "A".to_string(),
                    location: cols(7, 7)
                },
                kind: ClassKind::Regular,
                type_parameters: Some(TypeParameters {
                    values: vec![TypeParameter {
                        name: Constant {
                            source: None,
                            name: "B".to_string(),
                            location: cols(9, 9)
                        },
                        requirements: Some(Requirements {
                            values: vec![Requirement::Trait(TypeName {
                                name: Constant {
                                    source: Some(Identifier {
                                        name: "a".to_string(),
                                        location: cols(12, 12)
                                    }),
                                    name: "X".to_string(),
                                    location: cols(14, 14),
                                },
                                arguments: None,
                                location: cols(12, 14)
                            })],
                            location: cols(12, 14)
                        }),
                        location: cols(9, 14)
                    },],
                    location: cols(8, 15)
                }),
                body: ClassExpressions {
                    values: Vec::new(),
                    location: cols(17, 18)
                },
                location: cols(1, 18)
            }))
        );
    }

    #[test]
    fn test_class_with_instance_method() {
        assert_eq!(
            top(parse("class A { fn foo {} }")),
            TopLevelExpression::DefineClass(Box::new(DefineClass {
                public: false,
                name: Constant {
                    source: None,
                    name: "A".to_string(),
                    location: cols(7, 7)
                },
                kind: ClassKind::Regular,
                type_parameters: None,
                body: ClassExpressions {
                    values: vec![ClassExpression::DefineMethod(Box::new(
                        DefineMethod {
                            public: false,
                            operator: false,
                            kind: MethodKind::Instance,
                            name: Identifier {
                                name: "foo".to_string(),
                                location: cols(14, 16)
                            },
                            type_parameters: None,
                            arguments: None,
                            return_type: None,
                            body: Some(Expressions {
                                values: Vec::new(),
                                location: cols(18, 19)
                            }),
                            location: cols(11, 19)
                        }
                    ))],
                    location: cols(9, 21)
                },
                location: cols(1, 21)
            }))
        );

        assert_eq!(
            top(parse("class A { fn pub foo {} }")),
            TopLevelExpression::DefineClass(Box::new(DefineClass {
                public: false,
                name: Constant {
                    source: None,
                    name: "A".to_string(),
                    location: cols(7, 7)
                },
                kind: ClassKind::Regular,
                type_parameters: None,
                body: ClassExpressions {
                    values: vec![ClassExpression::DefineMethod(Box::new(
                        DefineMethod {
                            public: true,
                            operator: false,
                            kind: MethodKind::Instance,
                            name: Identifier {
                                name: "foo".to_string(),
                                location: cols(18, 20)
                            },
                            type_parameters: None,
                            arguments: None,
                            return_type: None,
                            body: Some(Expressions {
                                values: Vec::new(),
                                location: cols(22, 23)
                            }),
                            location: cols(11, 23)
                        }
                    ))],
                    location: cols(9, 25)
                },
                location: cols(1, 25)
            }))
        );
    }

    #[test]
    fn test_class_with_moving_method() {
        assert_eq!(
            top(parse("class A { fn move foo {} }")),
            TopLevelExpression::DefineClass(Box::new(DefineClass {
                public: false,
                name: Constant {
                    source: None,
                    name: "A".to_string(),
                    location: cols(7, 7)
                },
                kind: ClassKind::Regular,
                type_parameters: None,
                body: ClassExpressions {
                    values: vec![ClassExpression::DefineMethod(Box::new(
                        DefineMethod {
                            public: false,
                            operator: false,
                            kind: MethodKind::Moving,
                            name: Identifier {
                                name: "foo".to_string(),
                                location: cols(19, 21)
                            },
                            type_parameters: None,
                            arguments: None,
                            return_type: None,
                            body: Some(Expressions {
                                values: Vec::new(),
                                location: cols(23, 24)
                            }),
                            location: cols(11, 24)
                        }
                    ))],
                    location: cols(9, 26)
                },
                location: cols(1, 26)
            }))
        )
    }

    #[test]
    fn test_class_with_mutating_method() {
        assert_eq!(
            top(parse("class A { fn mut foo {} }")),
            TopLevelExpression::DefineClass(Box::new(DefineClass {
                public: false,
                name: Constant {
                    source: None,
                    name: "A".to_string(),
                    location: cols(7, 7)
                },
                kind: ClassKind::Regular,
                type_parameters: None,
                body: ClassExpressions {
                    values: vec![ClassExpression::DefineMethod(Box::new(
                        DefineMethod {
                            public: false,
                            operator: false,
                            kind: MethodKind::Mutable,
                            name: Identifier {
                                name: "foo".to_string(),
                                location: cols(18, 20)
                            },
                            type_parameters: None,
                            arguments: None,
                            return_type: None,
                            body: Some(Expressions {
                                values: Vec::new(),
                                location: cols(22, 23)
                            }),
                            location: cols(11, 23)
                        }
                    ))],
                    location: cols(9, 25)
                },
                location: cols(1, 25)
            }))
        )
    }

    #[test]
    fn test_class_with_static_method() {
        assert_eq!(
            top(parse("class A { fn static foo {} }")),
            TopLevelExpression::DefineClass(Box::new(DefineClass {
                public: false,
                name: Constant {
                    source: None,
                    name: "A".to_string(),
                    location: cols(7, 7)
                },
                kind: ClassKind::Regular,
                type_parameters: None,
                body: ClassExpressions {
                    values: vec![ClassExpression::DefineMethod(Box::new(
                        DefineMethod {
                            public: false,
                            operator: false,
                            kind: MethodKind::Static,
                            name: Identifier {
                                name: "foo".to_string(),
                                location: cols(21, 23)
                            },
                            type_parameters: None,
                            arguments: None,
                            return_type: None,
                            body: Some(Expressions {
                                values: Vec::new(),
                                location: cols(25, 26)
                            }),
                            location: cols(11, 26)
                        }
                    ))],
                    location: cols(9, 28)
                },
                location: cols(1, 28)
            }))
        )
    }

    #[test]
    fn test_class_with_field() {
        assert_eq!(
            top(parse("class A { let @foo: A }")),
            TopLevelExpression::DefineClass(Box::new(DefineClass {
                public: false,
                name: Constant {
                    source: None,
                    name: "A".to_string(),
                    location: cols(7, 7)
                },
                kind: ClassKind::Regular,
                type_parameters: None,
                body: ClassExpressions {
                    values: vec![ClassExpression::DefineField(Box::new(
                        DefineField {
                            public: false,
                            name: Identifier {
                                name: "foo".to_string(),
                                location: cols(15, 18)
                            },
                            value_type: Type::Named(Box::new(TypeName {
                                name: Constant {
                                    source: None,
                                    name: "A".to_string(),
                                    location: cols(21, 21)
                                },
                                arguments: None,
                                location: cols(21, 21)
                            })),
                            location: cols(11, 21)
                        }
                    ))],
                    location: cols(9, 23)
                },
                location: cols(1, 23)
            }))
        );

        assert_eq!(
            top(parse("class A { let pub @foo: A }")),
            TopLevelExpression::DefineClass(Box::new(DefineClass {
                public: false,
                name: Constant {
                    source: None,
                    name: "A".to_string(),
                    location: cols(7, 7)
                },
                kind: ClassKind::Regular,
                type_parameters: None,
                body: ClassExpressions {
                    values: vec![ClassExpression::DefineField(Box::new(
                        DefineField {
                            public: true,
                            name: Identifier {
                                name: "foo".to_string(),
                                location: cols(19, 22)
                            },
                            value_type: Type::Named(Box::new(TypeName {
                                name: Constant {
                                    source: None,
                                    name: "A".to_string(),
                                    location: cols(25, 25)
                                },
                                arguments: None,
                                location: cols(25, 25)
                            })),
                            location: cols(11, 25)
                        }
                    ))],
                    location: cols(9, 27)
                },
                location: cols(1, 27)
            }))
        );
    }

    #[test]
    fn test_invalid_classes() {
        assert_error!("class A { 10 }", cols(11, 12));
        assert_error!("class {}", cols(7, 7));
        assert_error!("class A {", cols(9, 9));
        assert_error!("class extern A[T] {", cols(15, 15));
        assert_error!("class extern A { fn foo {  } }", cols(18, 19));
    }

    #[test]
    fn test_implement_trait() {
        assert_eq!(
            top(parse("impl A for B {}")),
            TopLevelExpression::ImplementTrait(Box::new(ImplementTrait {
                trait_name: TypeName {
                    name: Constant {
                        source: None,
                        name: "A".to_string(),
                        location: cols(6, 6)
                    },
                    arguments: None,
                    location: cols(6, 6)
                },
                class_name: Constant {
                    source: None,
                    name: "B".to_string(),
                    location: cols(12, 12)
                },
                body: ImplementationExpressions {
                    values: Vec::new(),
                    location: cols(14, 15)
                },
                bounds: None,
                location: cols(1, 15)
            }))
        );

        assert_eq!(
            top(parse("impl A[B] for C {}")),
            TopLevelExpression::ImplementTrait(Box::new(ImplementTrait {
                trait_name: TypeName {
                    name: Constant {
                        source: None,
                        name: "A".to_string(),
                        location: cols(6, 6)
                    },
                    arguments: Some(Types {
                        values: vec![Type::Named(Box::new(TypeName {
                            name: Constant {
                                source: None,
                                name: "B".to_string(),
                                location: cols(8, 8)
                            },
                            arguments: None,
                            location: cols(8, 8)
                        }))],
                        location: cols(7, 9)
                    }),
                    location: cols(6, 9)
                },
                class_name: Constant {
                    source: None,
                    name: "C".to_string(),
                    location: cols(15, 15)
                },
                body: ImplementationExpressions {
                    values: Vec::new(),
                    location: cols(17, 18)
                },
                bounds: None,
                location: cols(1, 18)
            }))
        );

        assert_eq!(
            top(parse("impl A for B if X: A + B, Y: C + mut {}")),
            TopLevelExpression::ImplementTrait(Box::new(ImplementTrait {
                trait_name: TypeName {
                    name: Constant {
                        source: None,
                        name: "A".to_string(),
                        location: cols(6, 6)
                    },
                    arguments: None,
                    location: cols(6, 6)
                },
                class_name: Constant {
                    source: None,
                    name: "B".to_string(),
                    location: cols(12, 12)
                },
                body: ImplementationExpressions {
                    values: Vec::new(),
                    location: cols(38, 39)
                },
                bounds: Some(TypeBounds {
                    values: vec![
                        TypeBound {
                            name: Constant {
                                source: None,
                                name: "X".to_string(),
                                location: cols(17, 17)
                            },
                            requirements: Requirements {
                                values: vec![
                                    Requirement::Trait(TypeName {
                                        name: Constant {
                                            source: None,
                                            name: "A".to_string(),
                                            location: cols(20, 20)
                                        },
                                        arguments: None,
                                        location: cols(20, 20)
                                    }),
                                    Requirement::Trait(TypeName {
                                        name: Constant {
                                            source: None,
                                            name: "B".to_string(),
                                            location: cols(24, 24)
                                        },
                                        arguments: None,
                                        location: cols(24, 24)
                                    }),
                                ],
                                location: cols(20, 24)
                            },
                            location: cols(17, 24)
                        },
                        TypeBound {
                            name: Constant {
                                source: None,
                                name: "Y".to_string(),
                                location: cols(27, 27)
                            },
                            requirements: Requirements {
                                values: vec![
                                    Requirement::Trait(TypeName {
                                        name: Constant {
                                            source: None,
                                            name: "C".to_string(),
                                            location: cols(30, 30)
                                        },
                                        arguments: None,
                                        location: cols(30, 30)
                                    }),
                                    Requirement::Mutable(cols(34, 36))
                                ],
                                location: cols(30, 36)
                            },
                            location: cols(27, 36)
                        }
                    ],
                    location: cols(17, 36)
                }),
                location: cols(1, 39)
            }))
        );

        assert_eq!(
            top(parse("impl A for B { fn foo {} }")),
            TopLevelExpression::ImplementTrait(Box::new(ImplementTrait {
                trait_name: TypeName {
                    name: Constant {
                        source: None,
                        name: "A".to_string(),
                        location: cols(6, 6)
                    },
                    arguments: None,
                    location: cols(6, 6)
                },
                class_name: Constant {
                    source: None,
                    name: "B".to_string(),
                    location: cols(12, 12)
                },
                body: ImplementationExpressions {
                    values: vec![ImplementationExpression::DefineMethod(
                        Box::new(DefineMethod {
                            public: false,
                            operator: false,
                            kind: MethodKind::Instance,
                            name: Identifier {
                                name: "foo".to_string(),
                                location: cols(19, 21)
                            },
                            type_parameters: None,
                            arguments: None,
                            return_type: None,
                            body: Some(Expressions {
                                values: Vec::new(),
                                location: cols(23, 24)
                            }),
                            location: cols(16, 24)
                        })
                    )],
                    location: cols(14, 26)
                },
                bounds: None,
                location: cols(1, 26)
            }))
        );
    }

    #[test]
    fn test_reopen_class() {
        assert_eq!(
            top(parse("impl A {}")),
            TopLevelExpression::ReopenClass(Box::new(ReopenClass {
                class_name: Constant {
                    source: None,
                    name: "A".to_string(),
                    location: cols(6, 6)
                },
                body: ImplementationExpressions {
                    values: Vec::new(),
                    location: cols(8, 9)
                },
                bounds: None,
                location: cols(1, 9)
            }))
        );

        assert_eq!(
            top(parse("impl A { fn foo {} }")),
            TopLevelExpression::ReopenClass(Box::new(ReopenClass {
                class_name: Constant {
                    source: None,
                    name: "A".to_string(),
                    location: cols(6, 6)
                },
                body: ImplementationExpressions {
                    values: vec![ImplementationExpression::DefineMethod(
                        Box::new(DefineMethod {
                            public: false,
                            operator: false,
                            kind: MethodKind::Instance,
                            name: Identifier {
                                name: "foo".to_string(),
                                location: cols(13, 15)
                            },
                            type_parameters: None,
                            arguments: None,
                            return_type: None,
                            body: Some(Expressions {
                                values: Vec::new(),
                                location: cols(17, 18)
                            }),
                            location: cols(10, 18)
                        })
                    )],
                    location: cols(8, 20)
                },
                bounds: None,
                location: cols(1, 20)
            }))
        );

        assert_eq!(
            top(parse("impl A { fn async foo {} }")),
            TopLevelExpression::ReopenClass(Box::new(ReopenClass {
                class_name: Constant {
                    source: None,
                    name: "A".to_string(),
                    location: cols(6, 6)
                },
                body: ImplementationExpressions {
                    values: vec![ImplementationExpression::DefineMethod(
                        Box::new(DefineMethod {
                            public: false,
                            operator: false,
                            kind: MethodKind::Async,
                            name: Identifier {
                                name: "foo".to_string(),
                                location: cols(19, 21)
                            },
                            type_parameters: None,
                            arguments: None,
                            return_type: None,
                            body: Some(Expressions {
                                values: Vec::new(),
                                location: cols(23, 24)
                            }),
                            location: cols(10, 24)
                        })
                    )],
                    location: cols(8, 26)
                },
                bounds: None,
                location: cols(1, 26)
            }))
        );

        assert_eq!(
            top(parse("impl A if T: mut {}")),
            TopLevelExpression::ReopenClass(Box::new(ReopenClass {
                class_name: Constant {
                    source: None,
                    name: "A".to_string(),
                    location: cols(6, 6)
                },
                body: ImplementationExpressions {
                    values: Vec::new(),
                    location: cols(18, 19)
                },
                bounds: Some(TypeBounds {
                    values: vec![TypeBound {
                        name: Constant {
                            source: None,
                            name: "T".to_string(),
                            location: cols(11, 11)
                        },
                        requirements: Requirements {
                            values: vec![Requirement::Mutable(cols(14, 16))],
                            location: cols(14, 16)
                        },
                        location: cols(11, 16)
                    }],
                    location: cols(11, 16)
                }),
                location: cols(1, 16)
            }))
        );

        assert_eq!(
            top(parse("impl A if T: mut, {}")),
            TopLevelExpression::ReopenClass(Box::new(ReopenClass {
                class_name: Constant {
                    source: None,
                    name: "A".to_string(),
                    location: cols(6, 6)
                },
                body: ImplementationExpressions {
                    values: Vec::new(),
                    location: cols(19, 20)
                },
                bounds: Some(TypeBounds {
                    values: vec![TypeBound {
                        name: Constant {
                            source: None,
                            name: "T".to_string(),
                            location: cols(11, 11)
                        },
                        requirements: Requirements {
                            values: vec![Requirement::Mutable(cols(14, 16))],
                            location: cols(14, 16)
                        },
                        location: cols(11, 16)
                    }],
                    location: cols(11, 16)
                }),
                location: cols(1, 16)
            }))
        );
    }

    #[test]
    fn test_reopen_with_static_method() {
        assert_eq!(
            top(parse("impl A { fn static foo {} }")),
            TopLevelExpression::ReopenClass(Box::new(ReopenClass {
                class_name: Constant {
                    source: None,
                    name: "A".to_string(),
                    location: cols(6, 6)
                },
                body: ImplementationExpressions {
                    values: vec![ImplementationExpression::DefineMethod(
                        Box::new(DefineMethod {
                            public: false,
                            operator: false,
                            kind: MethodKind::Static,
                            name: Identifier {
                                name: "foo".to_string(),
                                location: cols(20, 22)
                            },
                            type_parameters: None,
                            arguments: None,
                            return_type: None,
                            body: Some(Expressions {
                                values: Vec::new(),
                                location: cols(24, 25)
                            }),
                            location: cols(10, 25)
                        })
                    )],
                    location: cols(8, 27)
                },
                bounds: None,
                location: cols(1, 27)
            }))
        );
    }

    #[test]
    fn test_invalid_implementations() {
        assert_error!("impl {}", cols(6, 6));
        assert_error!("impl A {", cols(8, 8));
        assert_error!("impl A { @foo: A }", cols(10, 13));
    }

    #[test]
    fn test_empty_trait() {
        assert_eq!(
            top(parse("trait A {}")),
            TopLevelExpression::DefineTrait(Box::new(DefineTrait {
                public: false,
                name: Constant {
                    source: None,
                    name: "A".to_string(),
                    location: cols(7, 7)
                },
                type_parameters: None,
                requirements: None,
                body: TraitExpressions {
                    values: Vec::new(),
                    location: cols(9, 10)
                },
                location: cols(1, 10)
            }))
        );

        assert_eq!(
            top(parse("trait pub A {}")),
            TopLevelExpression::DefineTrait(Box::new(DefineTrait {
                public: true,
                name: Constant {
                    source: None,
                    name: "A".to_string(),
                    location: cols(11, 11)
                },
                type_parameters: None,
                requirements: None,
                body: TraitExpressions {
                    values: Vec::new(),
                    location: cols(13, 14)
                },
                location: cols(1, 14)
            }))
        );
    }

    #[test]
    fn test_trait_with_requirements() {
        assert_eq!(
            top(parse("trait A: B + C {}")),
            TopLevelExpression::DefineTrait(Box::new(DefineTrait {
                public: false,
                name: Constant {
                    source: None,
                    name: "A".to_string(),
                    location: cols(7, 7)
                },
                type_parameters: None,
                requirements: Some(TypeNames {
                    values: vec![
                        TypeName {
                            name: Constant {
                                source: None,
                                name: "B".to_string(),
                                location: cols(10, 10)
                            },
                            arguments: None,
                            location: cols(10, 10)
                        },
                        TypeName {
                            name: Constant {
                                source: None,
                                name: "C".to_string(),
                                location: cols(14, 14)
                            },
                            arguments: None,
                            location: cols(14, 14)
                        },
                    ],
                    location: cols(10, 14)
                }),
                body: TraitExpressions {
                    values: Vec::new(),
                    location: cols(16, 17)
                },
                location: cols(1, 17)
            }))
        );

        assert_eq!(
            top(parse("trait A: a.B {}")),
            TopLevelExpression::DefineTrait(Box::new(DefineTrait {
                public: false,
                name: Constant {
                    source: None,
                    name: "A".to_string(),
                    location: cols(7, 7)
                },
                type_parameters: None,
                requirements: Some(TypeNames {
                    values: vec![TypeName {
                        name: Constant {
                            source: Some(Identifier {
                                name: "a".to_string(),
                                location: cols(10, 10)
                            }),
                            name: "B".to_string(),
                            location: cols(12, 12)
                        },
                        arguments: None,
                        location: cols(10, 12)
                    },],
                    location: cols(10, 12)
                }),
                body: TraitExpressions {
                    values: Vec::new(),
                    location: cols(14, 15)
                },
                location: cols(1, 15)
            }))
        );
    }

    #[test]
    fn test_trait_with_type_parameters() {
        assert_eq!(
            top(parse("trait A[B: X, C] {}")),
            TopLevelExpression::DefineTrait(Box::new(DefineTrait {
                public: false,
                name: Constant {
                    source: None,
                    name: "A".to_string(),
                    location: cols(7, 7)
                },
                type_parameters: Some(TypeParameters {
                    values: vec![
                        TypeParameter {
                            name: Constant {
                                source: None,
                                name: "B".to_string(),
                                location: cols(9, 9)
                            },
                            requirements: Some(Requirements {
                                values: vec![Requirement::Trait(TypeName {
                                    name: Constant {
                                        source: None,
                                        name: "X".to_string(),
                                        location: cols(12, 12),
                                    },
                                    arguments: None,
                                    location: cols(12, 12)
                                })],
                                location: cols(12, 12)
                            }),
                            location: cols(9, 12)
                        },
                        TypeParameter {
                            name: Constant {
                                source: None,
                                name: "C".to_string(),
                                location: cols(15, 15)
                            },
                            requirements: None,
                            location: cols(15, 15)
                        }
                    ],
                    location: cols(8, 16)
                }),
                requirements: None,
                body: TraitExpressions {
                    values: Vec::new(),
                    location: cols(18, 19)
                },
                location: cols(1, 19)
            }))
        );
    }

    #[test]
    fn test_trait_with_required_method() {
        assert_eq!(
            top(parse("trait A { fn foo }")),
            TopLevelExpression::DefineTrait(Box::new(DefineTrait {
                public: false,
                name: Constant {
                    source: None,
                    name: "A".to_string(),
                    location: cols(7, 7)
                },
                type_parameters: None,
                requirements: None,
                body: TraitExpressions {
                    values: vec![TraitExpression::DefineMethod(Box::new(
                        DefineMethod {
                            public: false,
                            operator: false,
                            kind: MethodKind::Instance,
                            name: Identifier {
                                name: "foo".to_string(),
                                location: cols(14, 16)
                            },
                            type_parameters: None,
                            arguments: None,
                            return_type: None,
                            body: None,
                            location: cols(11, 16)
                        }
                    ))],
                    location: cols(9, 18)
                },
                location: cols(1, 18)
            }))
        );
    }

    #[test]
    fn test_trait_with_required_method_with_bounds() {
        assert_eq!(
            top(parse("trait A { fn foo }")),
            TopLevelExpression::DefineTrait(Box::new(DefineTrait {
                public: false,
                name: Constant {
                    source: None,
                    name: "A".to_string(),
                    location: cols(7, 7)
                },
                type_parameters: None,
                requirements: None,
                body: TraitExpressions {
                    values: vec![TraitExpression::DefineMethod(Box::new(
                        DefineMethod {
                            public: false,
                            operator: false,
                            kind: MethodKind::Instance,
                            name: Identifier {
                                name: "foo".to_string(),
                                location: cols(14, 16)
                            },
                            type_parameters: None,
                            arguments: None,
                            return_type: None,
                            body: None,
                            location: cols(11, 16)
                        }
                    ))],
                    location: cols(9, 18)
                },
                location: cols(1, 18)
            }))
        );
    }

    #[test]
    fn test_trait_with_required_method_with_return_type() {
        assert_eq!(
            top(parse("trait A { fn foo -> A }")),
            TopLevelExpression::DefineTrait(Box::new(DefineTrait {
                public: false,
                name: Constant {
                    source: None,
                    name: "A".to_string(),
                    location: cols(7, 7)
                },
                type_parameters: None,
                requirements: None,
                body: TraitExpressions {
                    values: vec![TraitExpression::DefineMethod(Box::new(
                        DefineMethod {
                            public: false,
                            operator: false,
                            kind: MethodKind::Instance,
                            name: Identifier {
                                name: "foo".to_string(),
                                location: cols(14, 16)
                            },
                            type_parameters: None,
                            arguments: None,
                            return_type: Some(Type::Named(Box::new(
                                TypeName {
                                    name: Constant {
                                        source: None,
                                        name: "A".to_string(),
                                        location: cols(21, 21)
                                    },
                                    arguments: None,
                                    location: cols(21, 21)
                                }
                            ))),
                            body: None,
                            location: cols(11, 21)
                        }
                    ))],
                    location: cols(9, 23)
                },
                location: cols(1, 23)
            }))
        );
    }

    #[test]
    fn test_trait_with_required_method_with_arguments() {
        assert_eq!(
            top(parse("trait A { fn foo (a: A) }")),
            TopLevelExpression::DefineTrait(Box::new(DefineTrait {
                public: false,
                name: Constant {
                    source: None,
                    name: "A".to_string(),
                    location: cols(7, 7)
                },
                type_parameters: None,
                requirements: None,
                body: TraitExpressions {
                    values: vec![TraitExpression::DefineMethod(Box::new(
                        DefineMethod {
                            public: false,
                            operator: false,
                            kind: MethodKind::Instance,
                            name: Identifier {
                                name: "foo".to_string(),
                                location: cols(14, 16)
                            },
                            type_parameters: None,
                            arguments: Some(MethodArguments {
                                values: vec![MethodArgument {
                                    name: Identifier {
                                        name: "a".to_string(),
                                        location: cols(19, 19)
                                    },
                                    value_type: Type::Named(Box::new(
                                        TypeName {
                                            name: Constant {
                                                source: None,
                                                name: "A".to_string(),
                                                location: cols(22, 22)
                                            },
                                            arguments: None,
                                            location: cols(22, 22)
                                        }
                                    )),
                                    location: cols(19, 22)
                                }],
                                variadic: false,
                                location: cols(18, 23)
                            }),
                            return_type: None,
                            body: None,
                            location: cols(11, 23)
                        }
                    ))],
                    location: cols(9, 25)
                },
                location: cols(1, 25)
            }))
        );
    }

    #[test]
    fn test_trait_with_required_method_with_type_parameters() {
        assert_eq!(
            top(parse("trait A { fn foo [A] }")),
            TopLevelExpression::DefineTrait(Box::new(DefineTrait {
                public: false,
                name: Constant {
                    source: None,
                    name: "A".to_string(),
                    location: cols(7, 7)
                },
                type_parameters: None,
                requirements: None,
                body: TraitExpressions {
                    values: vec![TraitExpression::DefineMethod(Box::new(
                        DefineMethod {
                            public: false,
                            operator: false,
                            kind: MethodKind::Instance,
                            name: Identifier {
                                name: "foo".to_string(),
                                location: cols(14, 16)
                            },
                            type_parameters: Some(TypeParameters {
                                values: vec![TypeParameter {
                                    name: Constant {
                                        source: None,
                                        name: "A".to_string(),
                                        location: cols(19, 19)
                                    },
                                    requirements: None,
                                    location: cols(19, 19)
                                }],
                                location: cols(18, 20)
                            }),
                            arguments: None,
                            return_type: None,
                            body: None,
                            location: cols(11, 20)
                        }
                    ))],
                    location: cols(9, 22)
                },
                location: cols(1, 22)
            }))
        );
    }

    #[test]
    fn test_trait_with_default_method() {
        assert_eq!(
            top(parse("trait A { fn foo {} }")),
            TopLevelExpression::DefineTrait(Box::new(DefineTrait {
                public: false,
                name: Constant {
                    source: None,
                    name: "A".to_string(),
                    location: cols(7, 7)
                },
                type_parameters: None,
                requirements: None,
                body: TraitExpressions {
                    values: vec![TraitExpression::DefineMethod(Box::new(
                        DefineMethod {
                            public: false,
                            operator: false,
                            kind: MethodKind::Instance,
                            name: Identifier {
                                name: "foo".to_string(),
                                location: cols(14, 16)
                            },
                            type_parameters: None,
                            arguments: None,
                            return_type: None,
                            body: Some(Expressions {
                                values: Vec::new(),
                                location: cols(18, 19)
                            }),
                            location: cols(11, 19)
                        }
                    ))],
                    location: cols(9, 21)
                },
                location: cols(1, 21)
            }))
        );
    }

    #[test]
    fn test_trait_with_default_moving_method() {
        assert_eq!(
            top(parse("trait A { fn move foo {} }")),
            TopLevelExpression::DefineTrait(Box::new(DefineTrait {
                public: false,
                name: Constant {
                    source: None,
                    name: "A".to_string(),
                    location: cols(7, 7)
                },
                type_parameters: None,
                requirements: None,
                body: TraitExpressions {
                    values: vec![TraitExpression::DefineMethod(Box::new(
                        DefineMethod {
                            public: false,
                            operator: false,
                            kind: MethodKind::Moving,
                            name: Identifier {
                                name: "foo".to_string(),
                                location: cols(19, 21)
                            },
                            type_parameters: None,
                            arguments: None,
                            return_type: None,
                            body: Some(Expressions {
                                values: Vec::new(),
                                location: cols(23, 24)
                            }),
                            location: cols(11, 24)
                        }
                    ))],
                    location: cols(9, 26)
                },
                location: cols(1, 26)
            }))
        );
    }

    #[test]
    fn test_invalid_traits() {
        assert_error!("trait {}", cols(7, 7));
        assert_error!("trait A {", cols(9, 9));
        assert_error!("trait A { fn static a {} }", cols(21, 21));
        assert_error!("trait A { @foo: A }", cols(11, 14));
    }

    #[test]
    fn test_builtin_class() {
        assert_eq!(
            top(parse("class builtin A {}")),
            TopLevelExpression::DefineClass(Box::new(DefineClass {
                public: false,
                kind: ClassKind::Builtin,
                name: Constant {
                    source: None,
                    name: "A".to_string(),
                    location: cols(15, 15)
                },
                type_parameters: None,
                body: ClassExpressions {
                    values: Vec::new(),
                    location: cols(17, 18)
                },
                location: cols(1, 18)
            }))
        );
    }

    #[test]
    fn test_int_expression() {
        assert_eq!(
            expr("10"),
            Expression::Int(Box::new(IntLiteral {
                value: "10".to_string(),
                location: cols(1, 2)
            }))
        );

        assert_eq!(
            expr("1_0"),
            Expression::Int(Box::new(IntLiteral {
                value: "1_0".to_string(),
                location: cols(1, 3)
            }))
        );

        assert_eq!(
            expr("-10"),
            Expression::Int(Box::new(IntLiteral {
                value: "-10".to_string(),
                location: cols(1, 3)
            }))
        );
    }

    #[test]
    fn test_float_expression() {
        assert_eq!(
            expr("10.2"),
            Expression::Float(Box::new(FloatLiteral {
                value: "10.2".to_string(),
                location: cols(1, 4)
            }))
        );

        assert_eq!(
            expr("1_0.2"),
            Expression::Float(Box::new(FloatLiteral {
                value: "1_0.2".to_string(),
                location: cols(1, 5)
            }))
        );

        assert_eq!(
            expr("-10.2"),
            Expression::Float(Box::new(FloatLiteral {
                value: "-10.2".to_string(),
                location: cols(1, 5)
            }))
        );
    }

    #[test]
    fn test_invalid_single_string() {
        assert_error_expr!("'foo", cols(4, 4));
    }

    #[test]
    fn test_single_string_expression() {
        assert_eq!(
            expr("'foo'"),
            Expression::String(Box::new(StringLiteral {
                values: vec![StringValue::Text(Box::new(StringText {
                    value: "foo".to_string(),
                    location: cols(2, 4)
                }))],
                location: cols(1, 5)
            }))
        );

        assert_eq!(
            expr("'foo\nbar'"),
            Expression::String(Box::new(StringLiteral {
                values: vec![StringValue::Text(Box::new(StringText {
                    value: "foo\nbar".to_string(),
                    location: location(1..=2, 2..=3)
                }))],
                location: location(1..=2, 1..=4)
            }))
        );

        assert_eq!(
            expr("''"),
            Expression::String(Box::new(StringLiteral {
                values: vec![],
                location: cols(1, 2)
            }))
        );

        assert_eq!(
            expr("'foo${10 + 2}bar'"),
            Expression::String(Box::new(StringLiteral {
                values: vec![
                    StringValue::Text(Box::new(StringText {
                        value: "foo".to_string(),
                        location: cols(2, 4)
                    })),
                    StringValue::Expression(Box::new(StringExpression {
                        value: Expression::Binary(Box::new(Binary {
                            left: Expression::Int(Box::new(IntLiteral {
                                value: "10".to_string(),
                                location: cols(7, 8)
                            })),
                            right: Expression::Int(Box::new(IntLiteral {
                                value: "2".to_string(),
                                location: cols(12, 12)
                            })),
                            operator: Operator {
                                kind: OperatorKind::Add,
                                location: cols(10, 10)
                            },
                            location: cols(7, 12)
                        })),
                        location: cols(5, 13)
                    })),
                    StringValue::Text(Box::new(StringText {
                        value: "bar".to_string(),
                        location: cols(14, 16)
                    }))
                ],
                location: cols(1, 17)
            }))
        );

        assert_eq!(
            expr("'${'${10}'}'"),
            Expression::String(Box::new(StringLiteral {
                values: vec![StringValue::Expression(Box::new(
                    StringExpression {
                        value: Expression::String(Box::new(StringLiteral {
                            values: vec![StringValue::Expression(Box::new(
                                StringExpression {
                                    value: Expression::Int(Box::new(
                                        IntLiteral {
                                            value: "10".to_string(),
                                            location: location(1..=1, 7..=8)
                                        }
                                    )),
                                    location: cols(5, 9)
                                }
                            ))],
                            location: cols(4, 10)
                        })),
                        location: cols(2, 11)
                    }
                ))],
                location: cols(1, 12)
            }))
        );

        assert_eq!(
            expr("'foo\\u{AC}bar'"),
            Expression::String(Box::new(StringLiteral {
                values: vec![
                    StringValue::Text(Box::new(StringText {
                        value: "foo".to_string(),
                        location: location(1..=1, 2..=4)
                    })),
                    StringValue::Escape(Box::new(StringEscape {
                        value: "\u{AC}".to_string(),
                        location: location(1..=1, 5..=10)
                    })),
                    StringValue::Text(Box::new(StringText {
                        value: "bar".to_string(),
                        location: location(1..=1, 11..=13)
                    }))
                ],
                location: location(1..=1, 1..=14)
            }))
        );

        assert_eq!(
            expr("'foo\\u{AC}'"),
            Expression::String(Box::new(StringLiteral {
                values: vec![
                    StringValue::Text(Box::new(StringText {
                        value: "foo".to_string(),
                        location: location(1..=1, 2..=4)
                    })),
                    StringValue::Escape(Box::new(StringEscape {
                        value: "\u{AC}".to_string(),
                        location: location(1..=1, 5..=10)
                    }))
                ],
                location: location(1..=1, 1..=11)
            }))
        );

        assert_eq!(
            expr("'\\u{AC}bar'"),
            Expression::String(Box::new(StringLiteral {
                values: vec![
                    StringValue::Escape(Box::new(StringEscape {
                        value: "\u{AC}".to_string(),
                        location: location(1..=1, 2..=7)
                    })),
                    StringValue::Text(Box::new(StringText {
                        value: "bar".to_string(),
                        location: location(1..=1, 8..=10)
                    })),
                ],
                location: location(1..=1, 1..=11)
            }))
        );
    }

    #[test]
    fn test_double_string_expression() {
        assert_eq!(
            expr("\"foo\""),
            Expression::String(Box::new(StringLiteral {
                values: vec![StringValue::Text(Box::new(StringText {
                    value: "foo".to_string(),
                    location: cols(2, 4)
                }))],
                location: cols(1, 5)
            }))
        );

        assert_eq!(
            expr("\"foo\nbar\""),
            Expression::String(Box::new(StringLiteral {
                values: vec![StringValue::Text(Box::new(StringText {
                    value: "foo\nbar".to_string(),
                    location: location(1..=2, 2..=3)
                }))],
                location: location(1..=2, 1..=4)
            }))
        );

        assert_eq!(
            expr("\"\""),
            Expression::String(Box::new(StringLiteral {
                values: vec![],
                location: cols(1, 2)
            }))
        );

        assert_eq!(
            expr("\"foo${10 + 2}bar\""),
            Expression::String(Box::new(StringLiteral {
                values: vec![
                    StringValue::Text(Box::new(StringText {
                        value: "foo".to_string(),
                        location: cols(2, 4)
                    })),
                    StringValue::Expression(Box::new(StringExpression {
                        value: Expression::Binary(Box::new(Binary {
                            left: Expression::Int(Box::new(IntLiteral {
                                value: "10".to_string(),
                                location: cols(7, 8)
                            })),
                            right: Expression::Int(Box::new(IntLiteral {
                                value: "2".to_string(),
                                location: cols(12, 12)
                            })),
                            operator: Operator {
                                kind: OperatorKind::Add,
                                location: cols(10, 10)
                            },
                            location: cols(7, 12)
                        })),
                        location: cols(5, 13)
                    })),
                    StringValue::Text(Box::new(StringText {
                        value: "bar".to_string(),
                        location: cols(14, 16)
                    }))
                ],
                location: cols(1, 17)
            }))
        );

        assert_eq!(
            expr("\"${\"${10}\"}\""),
            Expression::String(Box::new(StringLiteral {
                values: vec![StringValue::Expression(Box::new(
                    StringExpression {
                        value: Expression::String(Box::new(StringLiteral {
                            values: vec![StringValue::Expression(Box::new(
                                StringExpression {
                                    value: Expression::Int(Box::new(
                                        IntLiteral {
                                            value: "10".to_string(),
                                            location: location(1..=1, 7..=8)
                                        }
                                    )),
                                    location: cols(5, 9)
                                }
                            ))],
                            location: cols(4, 10)
                        })),
                        location: cols(2, 11)
                    }
                ))],
                location: cols(1, 12)
            }))
        );

        assert_eq!(
            expr("\"foo\\u{AC}bar\""),
            Expression::String(Box::new(StringLiteral {
                values: vec![
                    StringValue::Text(Box::new(StringText {
                        value: "foo".to_string(),
                        location: location(1..=1, 2..=4)
                    })),
                    StringValue::Escape(Box::new(StringEscape {
                        value: "\u{AC}".to_string(),
                        location: location(1..=1, 5..=10)
                    })),
                    StringValue::Text(Box::new(StringText {
                        value: "bar".to_string(),
                        location: location(1..=1, 11..=13)
                    }))
                ],
                location: location(1..=1, 1..=14)
            }))
        );

        assert_eq!(
            expr("\"foo\\u{AC}\""),
            Expression::String(Box::new(StringLiteral {
                values: vec![
                    StringValue::Text(Box::new(StringText {
                        value: "foo".to_string(),
                        location: location(1..=1, 2..=4)
                    })),
                    StringValue::Escape(Box::new(StringEscape {
                        value: "\u{AC}".to_string(),
                        location: location(1..=1, 5..=10)
                    }))
                ],
                location: location(1..=1, 1..=11)
            }))
        );

        assert_eq!(
            expr("\"\\u{AC}bar\""),
            Expression::String(Box::new(StringLiteral {
                values: vec![
                    StringValue::Escape(Box::new(StringEscape {
                        value: "\u{AC}".to_string(),
                        location: location(1..=1, 2..=7)
                    })),
                    StringValue::Text(Box::new(StringText {
                        value: "bar".to_string(),
                        location: location(1..=1, 8..=10)
                    })),
                ],
                location: location(1..=1, 1..=11)
            }))
        );
    }

    #[test]
    fn test_invalid_double_string() {
        assert_error_expr!("\"foo", cols(4, 4));
        assert_error_expr!("\"foo${\"", cols(7, 7));
        assert_error_expr!("\"${}\"", cols(4, 4));
        assert_error_expr!("\"foo${\"${1}\"\"", cols(13, 13));
    }

    #[test]
    fn test_empty_array_expression() {
        assert_eq!(
            expr("[]"),
            Expression::Array(Box::new(Array {
                values: Vec::new(),
                location: cols(1, 2)
            }))
        );
    }

    #[test]
    fn test_array_expression() {
        assert_eq!(
            expr("[10, 20,]"),
            Expression::Array(Box::new(Array {
                values: vec![
                    Expression::Int(Box::new(IntLiteral {
                        value: "10".to_string(),
                        location: cols(2, 3)
                    })),
                    Expression::Int(Box::new(IntLiteral {
                        value: "20".to_string(),
                        location: cols(6, 7)
                    })),
                ],
                location: cols(1, 9)
            }))
        );
    }

    #[test]
    fn test_invalid_tuple() {
        assert_error_expr!("()", cols(2, 2));
        assert_error_expr!("(,)", cols(2, 2));
    }

    #[test]
    fn test_tuple_expression() {
        assert_eq!(
            expr("(10,)"),
            Expression::Tuple(Box::new(Tuple {
                values: vec![Expression::Int(Box::new(IntLiteral {
                    value: "10".to_string(),
                    location: cols(2, 3)
                })),],
                location: cols(1, 5)
            }))
        );

        assert_eq!(
            expr("(10, 20,)"),
            Expression::Tuple(Box::new(Tuple {
                values: vec![
                    Expression::Int(Box::new(IntLiteral {
                        value: "10".to_string(),
                        location: cols(2, 3)
                    })),
                    Expression::Int(Box::new(IntLiteral {
                        value: "20".to_string(),
                        location: cols(6, 7)
                    })),
                ],
                location: cols(1, 9)
            }))
        );
    }

    #[test]
    fn test_binary_expression() {
        assert_eq!(
            expr("10 + 2"),
            Expression::Binary(Box::new(Binary {
                operator: Operator {
                    kind: OperatorKind::Add,
                    location: cols(4, 4)
                },
                left: Expression::Int(Box::new(IntLiteral {
                    value: "10".to_string(),
                    location: cols(1, 2)
                })),
                right: Expression::Int(Box::new(IntLiteral {
                    value: "2".to_string(),
                    location: cols(6, 6)
                })),
                location: cols(1, 6)
            }))
        );

        assert_eq!(
            expr("10 - 2"),
            Expression::Binary(Box::new(Binary {
                operator: Operator {
                    kind: OperatorKind::Sub,
                    location: cols(4, 4)
                },
                left: Expression::Int(Box::new(IntLiteral {
                    value: "10".to_string(),
                    location: cols(1, 2)
                })),
                right: Expression::Int(Box::new(IntLiteral {
                    value: "2".to_string(),
                    location: cols(6, 6)
                })),
                location: cols(1, 6)
            }))
        );

        assert_eq!(
            expr("10 / 2"),
            Expression::Binary(Box::new(Binary {
                operator: Operator {
                    kind: OperatorKind::Div,
                    location: cols(4, 4)
                },
                left: Expression::Int(Box::new(IntLiteral {
                    value: "10".to_string(),
                    location: cols(1, 2)
                })),
                right: Expression::Int(Box::new(IntLiteral {
                    value: "2".to_string(),
                    location: cols(6, 6)
                })),
                location: cols(1, 6)
            }))
        );

        assert_eq!(
            expr("10 * 2"),
            Expression::Binary(Box::new(Binary {
                operator: Operator {
                    kind: OperatorKind::Mul,
                    location: cols(4, 4)
                },
                left: Expression::Int(Box::new(IntLiteral {
                    value: "10".to_string(),
                    location: cols(1, 2)
                })),
                right: Expression::Int(Box::new(IntLiteral {
                    value: "2".to_string(),
                    location: cols(6, 6)
                })),
                location: cols(1, 6)
            }))
        );

        assert_eq!(
            expr("10 ** 2"),
            Expression::Binary(Box::new(Binary {
                operator: Operator {
                    kind: OperatorKind::Pow,
                    location: cols(4, 5)
                },
                left: Expression::Int(Box::new(IntLiteral {
                    value: "10".to_string(),
                    location: cols(1, 2)
                })),
                right: Expression::Int(Box::new(IntLiteral {
                    value: "2".to_string(),
                    location: cols(7, 7)
                })),
                location: cols(1, 7)
            }))
        );

        assert_eq!(
            expr("10 % 2"),
            Expression::Binary(Box::new(Binary {
                operator: Operator {
                    kind: OperatorKind::Mod,
                    location: cols(4, 4)
                },
                left: Expression::Int(Box::new(IntLiteral {
                    value: "10".to_string(),
                    location: cols(1, 2)
                })),
                right: Expression::Int(Box::new(IntLiteral {
                    value: "2".to_string(),
                    location: cols(6, 6)
                })),
                location: cols(1, 6)
            }))
        );

        assert_eq!(
            expr("10 < 2"),
            Expression::Binary(Box::new(Binary {
                operator: Operator {
                    kind: OperatorKind::Lt,
                    location: cols(4, 4)
                },
                left: Expression::Int(Box::new(IntLiteral {
                    value: "10".to_string(),
                    location: cols(1, 2)
                })),
                right: Expression::Int(Box::new(IntLiteral {
                    value: "2".to_string(),
                    location: cols(6, 6)
                })),
                location: cols(1, 6)
            }))
        );

        assert_eq!(
            expr("10 > 2"),
            Expression::Binary(Box::new(Binary {
                operator: Operator {
                    kind: OperatorKind::Gt,
                    location: cols(4, 4)
                },
                left: Expression::Int(Box::new(IntLiteral {
                    value: "10".to_string(),
                    location: cols(1, 2)
                })),
                right: Expression::Int(Box::new(IntLiteral {
                    value: "2".to_string(),
                    location: cols(6, 6)
                })),
                location: cols(1, 6)
            }))
        );

        assert_eq!(
            expr("10 <= 2"),
            Expression::Binary(Box::new(Binary {
                operator: Operator {
                    kind: OperatorKind::Le,
                    location: cols(4, 5)
                },
                left: Expression::Int(Box::new(IntLiteral {
                    value: "10".to_string(),
                    location: cols(1, 2)
                })),
                right: Expression::Int(Box::new(IntLiteral {
                    value: "2".to_string(),
                    location: cols(7, 7)
                })),
                location: cols(1, 7)
            }))
        );

        assert_eq!(
            expr("10 >= 2"),
            Expression::Binary(Box::new(Binary {
                operator: Operator {
                    kind: OperatorKind::Ge,
                    location: cols(4, 5)
                },
                left: Expression::Int(Box::new(IntLiteral {
                    value: "10".to_string(),
                    location: cols(1, 2)
                })),
                right: Expression::Int(Box::new(IntLiteral {
                    value: "2".to_string(),
                    location: cols(7, 7)
                })),
                location: cols(1, 7)
            }))
        );

        assert_eq!(
            expr("10 << 2"),
            Expression::Binary(Box::new(Binary {
                operator: Operator {
                    kind: OperatorKind::Shl,
                    location: cols(4, 5)
                },
                left: Expression::Int(Box::new(IntLiteral {
                    value: "10".to_string(),
                    location: cols(1, 2)
                })),
                right: Expression::Int(Box::new(IntLiteral {
                    value: "2".to_string(),
                    location: cols(7, 7)
                })),
                location: cols(1, 7)
            }))
        );

        assert_eq!(
            expr("10 >> 2"),
            Expression::Binary(Box::new(Binary {
                operator: Operator {
                    kind: OperatorKind::Shr,
                    location: cols(4, 5)
                },
                left: Expression::Int(Box::new(IntLiteral {
                    value: "10".to_string(),
                    location: cols(1, 2)
                })),
                right: Expression::Int(Box::new(IntLiteral {
                    value: "2".to_string(),
                    location: cols(7, 7)
                })),
                location: cols(1, 7)
            }))
        );

        assert_eq!(
            expr("10 >>> 2"),
            Expression::Binary(Box::new(Binary {
                operator: Operator {
                    kind: OperatorKind::UnsignedShr,
                    location: cols(4, 6)
                },
                left: Expression::Int(Box::new(IntLiteral {
                    value: "10".to_string(),
                    location: cols(1, 2)
                })),
                right: Expression::Int(Box::new(IntLiteral {
                    value: "2".to_string(),
                    location: cols(8, 8)
                })),
                location: cols(1, 8)
            }))
        );

        assert_eq!(
            expr("10 & 2"),
            Expression::Binary(Box::new(Binary {
                operator: Operator {
                    kind: OperatorKind::BitAnd,
                    location: cols(4, 4)
                },
                left: Expression::Int(Box::new(IntLiteral {
                    value: "10".to_string(),
                    location: cols(1, 2)
                })),
                right: Expression::Int(Box::new(IntLiteral {
                    value: "2".to_string(),
                    location: cols(6, 6)
                })),
                location: cols(1, 6)
            }))
        );

        assert_eq!(
            expr("10 | 2"),
            Expression::Binary(Box::new(Binary {
                operator: Operator {
                    kind: OperatorKind::BitOr,
                    location: cols(4, 4)
                },
                left: Expression::Int(Box::new(IntLiteral {
                    value: "10".to_string(),
                    location: cols(1, 2)
                })),
                right: Expression::Int(Box::new(IntLiteral {
                    value: "2".to_string(),
                    location: cols(6, 6)
                })),
                location: cols(1, 6)
            }))
        );

        assert_eq!(
            expr("10 ^ 2"),
            Expression::Binary(Box::new(Binary {
                operator: Operator {
                    kind: OperatorKind::BitXor,
                    location: cols(4, 4)
                },
                left: Expression::Int(Box::new(IntLiteral {
                    value: "10".to_string(),
                    location: cols(1, 2)
                })),
                right: Expression::Int(Box::new(IntLiteral {
                    value: "2".to_string(),
                    location: cols(6, 6)
                })),
                location: cols(1, 6)
            }))
        );

        assert_eq!(
            expr("10 + 2 - 3"),
            Expression::Binary(Box::new(Binary {
                operator: Operator {
                    kind: OperatorKind::Sub,
                    location: cols(8, 8)
                },
                left: Expression::Binary(Box::new(Binary {
                    operator: Operator {
                        kind: OperatorKind::Add,
                        location: cols(4, 4)
                    },
                    left: Expression::Int(Box::new(IntLiteral {
                        value: "10".to_string(),
                        location: cols(1, 2)
                    })),
                    right: Expression::Int(Box::new(IntLiteral {
                        value: "2".to_string(),
                        location: cols(6, 6)
                    })),
                    location: cols(1, 6)
                })),
                right: Expression::Int(Box::new(IntLiteral {
                    value: "3".to_string(),
                    location: cols(10, 10)
                })),
                location: cols(1, 10)
            }))
        );

        assert_eq!(
            expr("10 == 2"),
            Expression::Binary(Box::new(Binary {
                operator: Operator {
                    kind: OperatorKind::Eq,
                    location: cols(4, 5)
                },
                left: Expression::Int(Box::new(IntLiteral {
                    value: "10".to_string(),
                    location: cols(1, 2)
                })),
                right: Expression::Int(Box::new(IntLiteral {
                    value: "2".to_string(),
                    location: cols(7, 7)
                })),
                location: cols(1, 7)
            }))
        );

        assert_eq!(
            expr("10 != 2"),
            Expression::Binary(Box::new(Binary {
                operator: Operator {
                    kind: OperatorKind::Ne,
                    location: cols(4, 5)
                },
                left: Expression::Int(Box::new(IntLiteral {
                    value: "10".to_string(),
                    location: cols(1, 2)
                })),
                right: Expression::Int(Box::new(IntLiteral {
                    value: "2".to_string(),
                    location: cols(7, 7)
                })),
                location: cols(1, 7)
            }))
        );
    }

    #[test]
    fn test_field_expression() {
        assert_eq!(
            expr("@foo"),
            Expression::Field(Box::new(Field {
                name: "foo".to_string(),
                location: cols(1, 4)
            }))
        );
    }

    #[test]
    fn test_constant_expression() {
        assert_eq!(
            expr("Foo"),
            Expression::Constant(Box::new(Constant {
                source: None,
                name: "Foo".to_string(),
                location: cols(1, 3)
            }))
        );
    }

    #[test]
    fn test_identifier_expression() {
        assert_eq!(
            expr("foo"),
            Expression::Identifier(Box::new(Identifier {
                name: "foo".to_string(),
                location: cols(1, 3)
            }))
        );
    }

    #[test]
    fn test_assign_expression() {
        assert_eq!(
            expr("foo = 10"),
            Expression::AssignVariable(Box::new(AssignVariable {
                variable: Identifier {
                    name: "foo".to_string(),
                    location: cols(1, 3)
                },
                value: Expression::Int(Box::new(IntLiteral {
                    value: "10".to_string(),
                    location: cols(7, 8)
                })),
                location: cols(1, 8)
            }))
        );

        assert_eq!(
            expr("foo\n= 10"),
            Expression::AssignVariable(Box::new(AssignVariable {
                variable: Identifier {
                    name: "foo".to_string(),
                    location: cols(1, 3)
                },
                value: Expression::Int(Box::new(IntLiteral {
                    value: "10".to_string(),
                    location: location(2..=2, 3..=4)
                })),
                location: location(1..=2, 1..=4)
            }))
        );

        assert_eq!(
            expr("foo =\n10"),
            Expression::AssignVariable(Box::new(AssignVariable {
                variable: Identifier {
                    name: "foo".to_string(),
                    location: cols(1, 3)
                },
                value: Expression::Int(Box::new(IntLiteral {
                    value: "10".to_string(),
                    location: location(2..=2, 1..=2)
                })),
                location: location(1..=2, 1..=2)
            }))
        );
    }

    #[test]
    fn test_replace_expression() {
        assert_eq!(
            expr("foo =: 10"),
            Expression::ReplaceVariable(Box::new(ReplaceVariable {
                variable: Identifier {
                    name: "foo".to_string(),
                    location: cols(1, 3)
                },
                value: Expression::Int(Box::new(IntLiteral {
                    value: "10".to_string(),
                    location: cols(8, 9)
                })),
                location: cols(1, 9)
            }))
        );
    }

    #[test]
    fn test_reassign_field_expression() {
        assert_eq!(
            expr("@foo = 10"),
            Expression::AssignField(Box::new(AssignField {
                field: Field { name: "foo".to_string(), location: cols(1, 4) },
                value: Expression::Int(Box::new(IntLiteral {
                    value: "10".to_string(),
                    location: cols(8, 9)
                })),
                location: cols(1, 9)
            }))
        );

        assert_eq!(
            expr("@foo\n= 10"),
            Expression::AssignField(Box::new(AssignField {
                field: Field { name: "foo".to_string(), location: cols(1, 4) },
                value: Expression::Int(Box::new(IntLiteral {
                    value: "10".to_string(),
                    location: location(2..=2, 3..=4)
                })),
                location: location(1..=2, 1..=4)
            }))
        );

        assert_eq!(
            expr("@foo =\n10"),
            Expression::AssignField(Box::new(AssignField {
                field: Field { name: "foo".to_string(), location: cols(1, 4) },
                value: Expression::Int(Box::new(IntLiteral {
                    value: "10".to_string(),
                    location: location(2..=2, 1..=2)
                })),
                location: location(1..=2, 1..=2)
            }))
        );
    }

    #[test]
    fn test_replace_field_expression() {
        assert_eq!(
            expr("@foo =: 10"),
            Expression::ReplaceField(Box::new(ReplaceField {
                field: Field { name: "foo".to_string(), location: cols(1, 4) },
                value: Expression::Int(Box::new(IntLiteral {
                    value: "10".to_string(),
                    location: cols(9, 10)
                })),
                location: cols(1, 10)
            }))
        );
    }

    #[test]
    fn test_binary_assign_expression() {
        assert_eq!(
            expr("foo += 10"),
            Expression::BinaryAssignVariable(Box::new(BinaryAssignVariable {
                variable: Identifier {
                    name: "foo".to_string(),
                    location: cols(1, 3)
                },
                value: Expression::Int(Box::new(IntLiteral {
                    value: "10".to_string(),
                    location: cols(8, 9)
                })),
                operator: Operator {
                    kind: OperatorKind::Add,
                    location: cols(5, 6)
                },
                location: cols(1, 9)
            }))
        );

        assert_eq!(
            expr("foo -= 10"),
            Expression::BinaryAssignVariable(Box::new(BinaryAssignVariable {
                variable: Identifier {
                    name: "foo".to_string(),
                    location: cols(1, 3)
                },
                value: Expression::Int(Box::new(IntLiteral {
                    value: "10".to_string(),
                    location: cols(8, 9)
                })),
                operator: Operator {
                    kind: OperatorKind::Sub,
                    location: cols(5, 6)
                },
                location: cols(1, 9)
            }))
        );

        assert_eq!(
            expr("foo /= 10"),
            Expression::BinaryAssignVariable(Box::new(BinaryAssignVariable {
                variable: Identifier {
                    name: "foo".to_string(),
                    location: cols(1, 3)
                },
                value: Expression::Int(Box::new(IntLiteral {
                    value: "10".to_string(),
                    location: cols(8, 9)
                })),
                operator: Operator {
                    kind: OperatorKind::Div,
                    location: cols(5, 6)
                },
                location: cols(1, 9)
            }))
        );

        assert_eq!(
            expr("foo *= 10"),
            Expression::BinaryAssignVariable(Box::new(BinaryAssignVariable {
                variable: Identifier {
                    name: "foo".to_string(),
                    location: cols(1, 3)
                },
                value: Expression::Int(Box::new(IntLiteral {
                    value: "10".to_string(),
                    location: cols(8, 9)
                })),
                operator: Operator {
                    kind: OperatorKind::Mul,
                    location: cols(5, 6)
                },
                location: cols(1, 9)
            }))
        );

        assert_eq!(
            expr("foo **= 10"),
            Expression::BinaryAssignVariable(Box::new(BinaryAssignVariable {
                variable: Identifier {
                    name: "foo".to_string(),
                    location: cols(1, 3)
                },
                value: Expression::Int(Box::new(IntLiteral {
                    value: "10".to_string(),
                    location: cols(9, 10)
                })),
                operator: Operator {
                    kind: OperatorKind::Pow,
                    location: cols(5, 7)
                },
                location: cols(1, 10)
            }))
        );

        assert_eq!(
            expr("foo %= 10"),
            Expression::BinaryAssignVariable(Box::new(BinaryAssignVariable {
                variable: Identifier {
                    name: "foo".to_string(),
                    location: cols(1, 3)
                },
                value: Expression::Int(Box::new(IntLiteral {
                    value: "10".to_string(),
                    location: cols(8, 9)
                })),
                operator: Operator {
                    kind: OperatorKind::Mod,
                    location: cols(5, 6)
                },
                location: cols(1, 9)
            }))
        );

        assert_eq!(
            expr("foo <<= 10"),
            Expression::BinaryAssignVariable(Box::new(BinaryAssignVariable {
                variable: Identifier {
                    name: "foo".to_string(),
                    location: cols(1, 3)
                },
                value: Expression::Int(Box::new(IntLiteral {
                    value: "10".to_string(),
                    location: cols(9, 10)
                })),
                operator: Operator {
                    kind: OperatorKind::Shl,
                    location: cols(5, 7)
                },
                location: cols(1, 10)
            }))
        );

        assert_eq!(
            expr("foo >>= 10"),
            Expression::BinaryAssignVariable(Box::new(BinaryAssignVariable {
                variable: Identifier {
                    name: "foo".to_string(),
                    location: cols(1, 3)
                },
                value: Expression::Int(Box::new(IntLiteral {
                    value: "10".to_string(),
                    location: cols(9, 10)
                })),
                operator: Operator {
                    kind: OperatorKind::Shr,
                    location: cols(5, 7)
                },
                location: cols(1, 10)
            }))
        );

        assert_eq!(
            expr("foo >>>= 10"),
            Expression::BinaryAssignVariable(Box::new(BinaryAssignVariable {
                variable: Identifier {
                    name: "foo".to_string(),
                    location: cols(1, 3)
                },
                value: Expression::Int(Box::new(IntLiteral {
                    value: "10".to_string(),
                    location: cols(10, 11)
                })),
                operator: Operator {
                    kind: OperatorKind::UnsignedShr,
                    location: cols(5, 8)
                },
                location: cols(1, 11)
            }))
        );

        assert_eq!(
            expr("foo &= 10"),
            Expression::BinaryAssignVariable(Box::new(BinaryAssignVariable {
                variable: Identifier {
                    name: "foo".to_string(),
                    location: cols(1, 3)
                },
                value: Expression::Int(Box::new(IntLiteral {
                    value: "10".to_string(),
                    location: cols(8, 9)
                })),
                operator: Operator {
                    kind: OperatorKind::BitAnd,
                    location: cols(5, 6)
                },
                location: cols(1, 9)
            }))
        );

        assert_eq!(
            expr("foo |= 10"),
            Expression::BinaryAssignVariable(Box::new(BinaryAssignVariable {
                variable: Identifier {
                    name: "foo".to_string(),
                    location: cols(1, 3)
                },
                value: Expression::Int(Box::new(IntLiteral {
                    value: "10".to_string(),
                    location: cols(8, 9)
                })),
                operator: Operator {
                    kind: OperatorKind::BitOr,
                    location: cols(5, 6)
                },
                location: cols(1, 9)
            }))
        );

        assert_eq!(
            expr("foo ^= 10"),
            Expression::BinaryAssignVariable(Box::new(BinaryAssignVariable {
                variable: Identifier {
                    name: "foo".to_string(),
                    location: cols(1, 3)
                },
                value: Expression::Int(Box::new(IntLiteral {
                    value: "10".to_string(),
                    location: cols(8, 9)
                })),
                operator: Operator {
                    kind: OperatorKind::BitXor,
                    location: cols(5, 6)
                },
                location: cols(1, 9)
            }))
        );
    }

    #[test]
    fn test_binary_assign_field_expression() {
        assert_eq!(
            expr("@foo += 10"),
            Expression::BinaryAssignField(Box::new(BinaryAssignField {
                field: Field { name: "foo".to_string(), location: cols(1, 4) },
                value: Expression::Int(Box::new(IntLiteral {
                    value: "10".to_string(),
                    location: cols(9, 10)
                })),
                operator: Operator {
                    kind: OperatorKind::Add,
                    location: cols(6, 7)
                },
                location: cols(1, 10)
            }))
        );

        assert_eq!(
            expr("@foo -= 10"),
            Expression::BinaryAssignField(Box::new(BinaryAssignField {
                field: Field { name: "foo".to_string(), location: cols(1, 4) },
                value: Expression::Int(Box::new(IntLiteral {
                    value: "10".to_string(),
                    location: cols(9, 10)
                })),
                operator: Operator {
                    kind: OperatorKind::Sub,
                    location: cols(6, 7)
                },
                location: cols(1, 10)
            }))
        );

        assert_eq!(
            expr("@foo /= 10"),
            Expression::BinaryAssignField(Box::new(BinaryAssignField {
                field: Field { name: "foo".to_string(), location: cols(1, 4) },
                value: Expression::Int(Box::new(IntLiteral {
                    value: "10".to_string(),
                    location: cols(9, 10)
                })),
                operator: Operator {
                    kind: OperatorKind::Div,
                    location: cols(6, 7)
                },
                location: cols(1, 10)
            }))
        );

        assert_eq!(
            expr("@foo *= 10"),
            Expression::BinaryAssignField(Box::new(BinaryAssignField {
                field: Field { name: "foo".to_string(), location: cols(1, 4) },
                value: Expression::Int(Box::new(IntLiteral {
                    value: "10".to_string(),
                    location: cols(9, 10)
                })),
                operator: Operator {
                    kind: OperatorKind::Mul,
                    location: cols(6, 7)
                },
                location: cols(1, 10)
            }))
        );

        assert_eq!(
            expr("@foo **= 10"),
            Expression::BinaryAssignField(Box::new(BinaryAssignField {
                field: Field { name: "foo".to_string(), location: cols(1, 4) },
                value: Expression::Int(Box::new(IntLiteral {
                    value: "10".to_string(),
                    location: cols(10, 11)
                })),
                operator: Operator {
                    kind: OperatorKind::Pow,
                    location: cols(6, 8)
                },
                location: cols(1, 11)
            }))
        );

        assert_eq!(
            expr("@foo %= 10"),
            Expression::BinaryAssignField(Box::new(BinaryAssignField {
                field: Field { name: "foo".to_string(), location: cols(1, 4) },
                value: Expression::Int(Box::new(IntLiteral {
                    value: "10".to_string(),
                    location: cols(9, 10)
                })),
                operator: Operator {
                    kind: OperatorKind::Mod,
                    location: cols(6, 7)
                },
                location: cols(1, 10)
            }))
        );

        assert_eq!(
            expr("@foo <<= 10"),
            Expression::BinaryAssignField(Box::new(BinaryAssignField {
                field: Field { name: "foo".to_string(), location: cols(1, 4) },
                value: Expression::Int(Box::new(IntLiteral {
                    value: "10".to_string(),
                    location: cols(10, 11)
                })),
                operator: Operator {
                    kind: OperatorKind::Shl,
                    location: cols(6, 8)
                },
                location: cols(1, 11)
            }))
        );

        assert_eq!(
            expr("@foo >>= 10"),
            Expression::BinaryAssignField(Box::new(BinaryAssignField {
                field: Field { name: "foo".to_string(), location: cols(1, 4) },
                value: Expression::Int(Box::new(IntLiteral {
                    value: "10".to_string(),
                    location: cols(10, 11)
                })),
                operator: Operator {
                    kind: OperatorKind::Shr,
                    location: cols(6, 8)
                },
                location: cols(1, 11)
            }))
        );

        assert_eq!(
            expr("@foo >>>= 10"),
            Expression::BinaryAssignField(Box::new(BinaryAssignField {
                field: Field { name: "foo".to_string(), location: cols(1, 4) },
                value: Expression::Int(Box::new(IntLiteral {
                    value: "10".to_string(),
                    location: cols(11, 12)
                })),
                operator: Operator {
                    kind: OperatorKind::UnsignedShr,
                    location: cols(6, 9)
                },
                location: cols(1, 12)
            }))
        );

        assert_eq!(
            expr("@foo &= 10"),
            Expression::BinaryAssignField(Box::new(BinaryAssignField {
                field: Field { name: "foo".to_string(), location: cols(1, 4) },
                value: Expression::Int(Box::new(IntLiteral {
                    value: "10".to_string(),
                    location: cols(9, 10)
                })),
                operator: Operator {
                    kind: OperatorKind::BitAnd,
                    location: cols(6, 7)
                },
                location: cols(1, 10)
            }))
        );

        assert_eq!(
            expr("@foo |= 10"),
            Expression::BinaryAssignField(Box::new(BinaryAssignField {
                field: Field { name: "foo".to_string(), location: cols(1, 4) },
                value: Expression::Int(Box::new(IntLiteral {
                    value: "10".to_string(),
                    location: cols(9, 10)
                })),
                operator: Operator {
                    kind: OperatorKind::BitOr,
                    location: cols(6, 7)
                },
                location: cols(1, 10)
            }))
        );

        assert_eq!(
            expr("@foo ^= 10"),
            Expression::BinaryAssignField(Box::new(BinaryAssignField {
                field: Field { name: "foo".to_string(), location: cols(1, 4) },
                value: Expression::Int(Box::new(IntLiteral {
                    value: "10".to_string(),
                    location: cols(9, 10)
                })),
                operator: Operator {
                    kind: OperatorKind::BitXor,
                    location: cols(6, 7)
                },
                location: cols(1, 10)
            }))
        );
    }

    #[test]
    fn test_invalid_reassigns() {
        assert_error_expr!("foo = ", cols(6, 6));
        assert_error_expr!("foo = }", cols(7, 7));
    }

    #[test]
    fn test_calls() {
        assert_eq!(
            expr("foo()"),
            Expression::Call(Box::new(Call {
                receiver: None,
                arguments: Some(Arguments {
                    values: Vec::new(),
                    location: cols(4, 5)
                }),
                name: Identifier {
                    name: "foo".to_string(),
                    location: cols(1, 3)
                },
                location: cols(1, 5)
            }))
        );

        assert_eq!(
            expr("Foo()"),
            Expression::Call(Box::new(Call {
                receiver: None,
                arguments: Some(Arguments {
                    values: Vec::new(),
                    location: cols(4, 5)
                }),
                name: Identifier {
                    name: "Foo".to_string(),
                    location: cols(1, 3)
                },
                location: cols(1, 5)
            }))
        );

        assert_eq!(
            expr("foo(10, 20)"),
            Expression::Call(Box::new(Call {
                receiver: None,
                name: Identifier {
                    name: "foo".to_string(),
                    location: cols(1, 3)
                },
                arguments: Some(Arguments {
                    values: vec![
                        Argument::Positional(Expression::Int(Box::new(
                            IntLiteral {
                                value: "10".to_string(),
                                location: cols(5, 6)
                            }
                        ))),
                        Argument::Positional(Expression::Int(Box::new(
                            IntLiteral {
                                value: "20".to_string(),
                                location: cols(9, 10)
                            }
                        ))),
                    ],
                    location: cols(4, 11)
                }),
                location: cols(1, 11)
            }))
        );

        assert_eq!(
            expr("foo(ab: 10)"),
            Expression::Call(Box::new(Call {
                receiver: None,
                name: Identifier {
                    name: "foo".to_string(),
                    location: cols(1, 3)
                },
                arguments: Some(Arguments {
                    values: vec![Argument::Named(Box::new(NamedArgument {
                        name: Identifier {
                            name: "ab".to_string(),
                            location: cols(5, 6)
                        },
                        value: Expression::Int(Box::new(IntLiteral {
                            value: "10".to_string(),
                            location: cols(9, 10)
                        })),
                        location: cols(5, 10)
                    })),],
                    location: cols(4, 11)
                }),
                location: cols(1, 11)
            }))
        );

        assert_eq!(
            expr("foo(class: 10)"),
            Expression::Call(Box::new(Call {
                receiver: None,
                name: Identifier {
                    name: "foo".to_string(),
                    location: cols(1, 3)
                },
                arguments: Some(Arguments {
                    values: vec![Argument::Named(Box::new(NamedArgument {
                        name: Identifier {
                            name: "class".to_string(),
                            location: cols(5, 9)
                        },
                        value: Expression::Int(Box::new(IntLiteral {
                            value: "10".to_string(),
                            location: cols(12, 13)
                        })),
                        location: cols(5, 13)
                    })),],
                    location: cols(4, 14)
                }),
                location: cols(1, 14)
            }))
        );
    }

    #[test]
    fn test_call_with_trailing_blocks_without_parentheses() {
        assert_eq!(
            expr("foo fn {}"),
            Expression::Call(Box::new(Call {
                receiver: None,
                name: Identifier {
                    name: "foo".to_string(),
                    location: cols(1, 3)
                },
                arguments: Some(Arguments {
                    values: vec![Argument::Positional(Expression::Closure(
                        Box::new(Closure {
                            moving: false,
                            arguments: None,
                            return_type: None,
                            body: Expressions {
                                values: vec![],
                                location: cols(8, 9)
                            },
                            location: cols(5, 9)
                        })
                    ))],
                    location: cols(5, 9)
                }),
                location: cols(1, 9)
            }))
        );
    }

    #[test]
    fn test_call_with_receiver_with_trailing_blocks_without_parentheses() {
        assert_eq!(
            expr("10.foo fn {}"),
            Expression::Call(Box::new(Call {
                receiver: Some(Expression::Int(Box::new(IntLiteral {
                    value: "10".to_string(),
                    location: cols(1, 2)
                }))),
                name: Identifier {
                    name: "foo".to_string(),
                    location: cols(4, 6)
                },
                arguments: Some(Arguments {
                    values: vec![Argument::Positional(Expression::Closure(
                        Box::new(Closure {
                            moving: false,
                            arguments: None,
                            return_type: None,
                            body: Expressions {
                                values: vec![],
                                location: cols(11, 12)
                            },
                            location: cols(8, 12)
                        })
                    ))],
                    location: cols(8, 12)
                }),
                location: cols(1, 12)
            }))
        );
    }

    #[test]
    fn test_call_with_receiver_with_trailing_blocks_with_parentheses() {
        assert_eq!(
            expr("10.foo() fn {}"),
            Expression::Call(Box::new(Call {
                receiver: Some(Expression::Int(Box::new(IntLiteral {
                    value: "10".to_string(),
                    location: cols(1, 2)
                }))),
                name: Identifier {
                    name: "foo".to_string(),
                    location: cols(4, 6)
                },
                arguments: Some(Arguments {
                    values: vec![Argument::Positional(Expression::Closure(
                        Box::new(Closure {
                            moving: false,
                            arguments: None,
                            return_type: None,
                            body: Expressions {
                                values: vec![],
                                location: cols(13, 14)
                            },
                            location: cols(10, 14)
                        })
                    ))],
                    location: cols(7, 8)
                }),
                location: cols(1, 8)
            }))
        );
    }

    #[test]
    fn test_call_with_trailing_blocks_with_parentheses() {
        assert_eq!(
            expr("foo() fn {}"),
            Expression::Call(Box::new(Call {
                receiver: None,
                name: Identifier {
                    name: "foo".to_string(),
                    location: cols(1, 3)
                },
                arguments: Some(Arguments {
                    values: vec![Argument::Positional(Expression::Closure(
                        Box::new(Closure {
                            moving: false,
                            arguments: None,
                            return_type: None,
                            body: Expressions {
                                values: vec![],
                                location: cols(10, 11)
                            },
                            location: cols(7, 11)
                        })
                    ))],
                    location: cols(4, 5)
                }),
                location: cols(1, 5)
            }))
        );

        assert_eq!(
            expr("foo(\n) fn {}"),
            Expression::Call(Box::new(Call {
                receiver: None,
                name: Identifier {
                    name: "foo".to_string(),
                    location: cols(1, 3)
                },
                arguments: Some(Arguments {
                    values: vec![Argument::Positional(Expression::Closure(
                        Box::new(Closure {
                            moving: false,
                            arguments: None,
                            return_type: None,
                            body: Expressions {
                                values: vec![],
                                location: location(2..=2, 6..=7)
                            },
                            location: location(2..=2, 3..=7)
                        })
                    ))],
                    location: location(1..=2, 4..=1)
                }),
                location: location(1..=2, 1..=1)
            }))
        );
    }

    #[test]
    fn test_call_with_block_on_new_line() {
        let mut parser = parser("foo\nfn {}");
        let token1 = parser.require().unwrap();
        let node1 = parser.expression(token1).unwrap();
        let token2 = parser.require().unwrap();
        let node2 = parser.expression(token2).unwrap();

        assert_eq!(
            node1,
            Expression::Identifier(Box::new(Identifier {
                name: "foo".to_string(),
                location: cols(1, 3)
            }))
        );

        assert_eq!(
            node2,
            Expression::Closure(Box::new(Closure {
                moving: false,
                arguments: None,
                return_type: None,
                body: Expressions {
                    values: Vec::new(),
                    location: location(2..=2, 4..=5)
                },
                location: location(2..=2, 1..=5)
            }))
        );
    }

    #[test]
    fn test_calls_with_receivers() {
        assert_eq!(
            expr("10.foo()"),
            Expression::Call(Box::new(Call {
                receiver: Some(Expression::Int(Box::new(IntLiteral {
                    value: "10".to_string(),
                    location: cols(1, 2)
                }))),
                name: Identifier {
                    name: "foo".to_string(),
                    location: cols(4, 6)
                },
                arguments: Some(Arguments {
                    values: Vec::new(),
                    location: cols(7, 8)
                }),
                location: cols(1, 8)
            }))
        );

        assert_eq!(
            expr("10.Foo()"),
            Expression::Call(Box::new(Call {
                receiver: Some(Expression::Int(Box::new(IntLiteral {
                    value: "10".to_string(),
                    location: cols(1, 2)
                }))),
                name: Identifier {
                    name: "Foo".to_string(),
                    location: cols(4, 6)
                },
                arguments: Some(Arguments {
                    values: Vec::new(),
                    location: cols(7, 8)
                }),
                location: cols(1, 8)
            }))
        );

        assert_eq!(
            expr("10.foo"),
            Expression::Call(Box::new(Call {
                receiver: Some(Expression::Int(Box::new(IntLiteral {
                    value: "10".to_string(),
                    location: cols(1, 2)
                }))),
                name: Identifier {
                    name: "foo".to_string(),
                    location: cols(4, 6)
                },
                arguments: None,
                location: cols(1, 6)
            }))
        );

        assert_eq!(
            expr("ab.123"),
            Expression::Call(Box::new(Call {
                receiver: Some(Expression::Identifier(Box::new(Identifier {
                    name: "ab".to_string(),
                    location: cols(1, 2)
                }))),
                name: Identifier {
                    name: "123".to_string(),
                    location: cols(4, 6)
                },
                arguments: None,
                location: cols(1, 6)
            }))
        );

        assert_eq!(
            expr("10.try"),
            Expression::Call(Box::new(Call {
                receiver: Some(Expression::Int(Box::new(IntLiteral {
                    value: "10".to_string(),
                    location: cols(1, 2)
                }))),
                name: Identifier {
                    name: "try".to_string(),
                    location: cols(4, 6)
                },
                arguments: None,
                location: cols(1, 6)
            }))
        );

        assert_eq!(
            expr("10.foo = 20"),
            Expression::AssignSetter(Box::new(AssignSetter {
                receiver: Expression::Int(Box::new(IntLiteral {
                    value: "10".to_string(),
                    location: cols(1, 2)
                })),
                name: Identifier {
                    name: "foo".to_string(),
                    location: cols(4, 6)
                },
                value: Expression::Int(Box::new(IntLiteral {
                    value: "20".to_string(),
                    location: cols(10, 11)
                })),
                location: cols(1, 11)
            }))
        );

        assert_eq!(
            expr("10.foo += 20"),
            Expression::BinaryAssignSetter(Box::new(BinaryAssignSetter {
                receiver: Expression::Int(Box::new(IntLiteral {
                    value: "10".to_string(),
                    location: cols(1, 2)
                })),
                name: Identifier {
                    name: "foo".to_string(),
                    location: cols(4, 6)
                },
                operator: Operator {
                    kind: OperatorKind::Add,
                    location: cols(8, 9)
                },
                value: Expression::Int(Box::new(IntLiteral {
                    value: "20".to_string(),
                    location: cols(11, 12)
                })),
                location: cols(1, 12)
            }))
        );

        assert_eq!(
            expr("10.foo.bar"),
            Expression::Call(Box::new(Call {
                receiver: Some(Expression::Call(Box::new(Call {
                    receiver: Some(Expression::Int(Box::new(IntLiteral {
                        value: "10".to_string(),
                        location: cols(1, 2)
                    }))),
                    name: Identifier {
                        name: "foo".to_string(),
                        location: cols(4, 6)
                    },
                    arguments: None,
                    location: cols(1, 6)
                }))),
                name: Identifier {
                    name: "bar".to_string(),
                    location: cols(8, 10)
                },
                arguments: None,
                location: cols(1, 10)
            }))
        );
    }

    #[test]
    fn test_invalid_calls() {
        assert_error_expr!("foo(", cols(4, 4));
        assert_error_expr!("foo(a: 10, 20)", cols(12, 13));
        assert_error_expr!("10.foo =", cols(8, 8));
    }

    #[test]
    fn test_scope() {
        assert_eq!(
            expr("{ 10 }"),
            Expression::Scope(Box::new(Scope {
                body: Expressions {
                    values: vec![Expression::Int(Box::new(IntLiteral {
                        value: "10".to_string(),
                        location: cols(3, 4)
                    }))],
                    location: cols(1, 6)
                },
                location: cols(1, 6)
            }))
        );
    }

    #[test]
    fn test_closures() {
        assert_eq!(
            expr("fn { 10 }"),
            Expression::Closure(Box::new(Closure {
                moving: false,
                arguments: None,
                return_type: None,
                body: Expressions {
                    values: vec![Expression::Int(Box::new(IntLiteral {
                        value: "10".to_string(),
                        location: cols(6, 7)
                    }))],
                    location: cols(4, 9)
                },
                location: cols(1, 9)
            }))
        );

        assert_eq!(
            expr("fn move { 10 }"),
            Expression::Closure(Box::new(Closure {
                moving: true,
                arguments: None,
                return_type: None,
                body: Expressions {
                    values: vec![Expression::Int(Box::new(IntLiteral {
                        value: "10".to_string(),
                        location: cols(11, 12)
                    }))],
                    location: cols(9, 14)
                },
                location: cols(1, 14)
            }))
        );

        assert_eq!(
            expr("fn (a) { 10 }"),
            Expression::Closure(Box::new(Closure {
                moving: false,
                arguments: Some(BlockArguments {
                    values: vec![BlockArgument {
                        name: Identifier {
                            name: "a".to_string(),
                            location: cols(5, 5)
                        },
                        value_type: None,
                        location: cols(5, 5)
                    }],
                    location: cols(4, 6)
                }),
                return_type: None,
                body: Expressions {
                    values: vec![Expression::Int(Box::new(IntLiteral {
                        value: "10".to_string(),
                        location: cols(10, 11)
                    }))],
                    location: cols(8, 13)
                },
                location: cols(1, 13)
            }))
        );

        assert_eq!(
            expr("fn (a: T) { 10 }"),
            Expression::Closure(Box::new(Closure {
                moving: false,
                arguments: Some(BlockArguments {
                    values: vec![BlockArgument {
                        name: Identifier {
                            name: "a".to_string(),
                            location: cols(5, 5)
                        },
                        value_type: Some(Type::Named(Box::new(TypeName {
                            name: Constant {
                                source: None,
                                name: "T".to_string(),
                                location: cols(8, 8)
                            },
                            arguments: None,
                            location: cols(8, 8)
                        }))),
                        location: cols(5, 8)
                    }],
                    location: cols(4, 9)
                }),
                return_type: None,
                body: Expressions {
                    values: vec![Expression::Int(Box::new(IntLiteral {
                        value: "10".to_string(),
                        location: cols(13, 14)
                    }))],
                    location: cols(11, 16)
                },
                location: cols(1, 16)
            }))
        );

        assert_eq!(
            expr("fn -> T { 10 }"),
            Expression::Closure(Box::new(Closure {
                moving: false,
                arguments: None,
                return_type: Some(Type::Named(Box::new(TypeName {
                    name: Constant {
                        source: None,
                        name: "T".to_string(),
                        location: cols(7, 7)
                    },
                    arguments: None,
                    location: cols(7, 7)
                }))),
                body: Expressions {
                    values: vec![Expression::Int(Box::new(IntLiteral {
                        value: "10".to_string(),
                        location: cols(11, 12)
                    }))],
                    location: cols(9, 14)
                },
                location: cols(1, 14)
            }))
        );
    }

    #[test]
    fn test_invalid_closures() {
        assert_error_expr!("fn {", cols(4, 4));
        assert_error_expr!("fn ->", cols(5, 5));
        assert_error_expr!("fn =>", cols(4, 5));
    }

    #[test]
    fn test_variables() {
        assert_eq!(
            expr("let x = 10"),
            Expression::DefineVariable(Box::new(DefineVariable {
                mutable: false,
                value_type: None,
                name: Identifier {
                    name: "x".to_string(),
                    location: cols(5, 5)
                },
                value: Expression::Int(Box::new(IntLiteral {
                    value: "10".to_string(),
                    location: cols(9, 10)
                })),
                location: cols(1, 10)
            }))
        );

        assert_eq!(
            expr("let x: A = 10"),
            Expression::DefineVariable(Box::new(DefineVariable {
                name: Identifier {
                    name: "x".to_string(),
                    location: cols(5, 5)
                },
                mutable: false,
                value_type: Some(Type::Named(Box::new(TypeName {
                    name: Constant {
                        source: None,
                        name: "A".to_string(),
                        location: cols(8, 8)
                    },
                    arguments: None,
                    location: cols(8, 8)
                }))),
                value: Expression::Int(Box::new(IntLiteral {
                    value: "10".to_string(),
                    location: cols(12, 13)
                })),
                location: cols(1, 13)
            }))
        );

        assert_eq!(
            expr("let mut x = 10"),
            Expression::DefineVariable(Box::new(DefineVariable {
                name: Identifier {
                    name: "x".to_string(),
                    location: cols(9, 9)
                },
                mutable: true,
                value_type: None,
                value: Expression::Int(Box::new(IntLiteral {
                    value: "10".to_string(),
                    location: cols(13, 14)
                })),
                location: cols(1, 14)
            }))
        );
    }

    #[test]
    fn test_self_expression() {
        assert_eq!(
            expr("self"),
            Expression::SelfObject(Box::new(SelfObject {
                location: cols(1, 4)
            }))
        );
    }

    #[test]
    fn test_nil_expression() {
        assert_eq!(
            expr("nil"),
            Expression::Nil(Box::new(Nil { location: cols(1, 3) }))
        );
    }

    #[test]
    fn test_true_expression() {
        assert_eq!(
            expr("true"),
            Expression::True(Box::new(True { location: cols(1, 4) }))
        );
    }

    #[test]
    fn test_false_expression() {
        assert_eq!(
            expr("false"),
            Expression::False(Box::new(False { location: cols(1, 5) }))
        );
    }

    #[test]
    fn test_grouped_expression() {
        assert_eq!(
            expr("(self)"),
            Expression::Group(Box::new(Group {
                value: Expression::SelfObject(Box::new(SelfObject {
                    location: cols(2, 5)
                })),
                location: cols(1, 6)
            }))
        );

        assert_eq!(
            expr("1 + (2 + 3)"),
            Expression::Binary(Box::new(Binary {
                operator: Operator {
                    kind: OperatorKind::Add,
                    location: cols(3, 3)
                },
                left: Expression::Int(Box::new(IntLiteral {
                    value: "1".to_string(),
                    location: cols(1, 1)
                })),
                right: Expression::Group(Box::new(Group {
                    value: Expression::Binary(Box::new(Binary {
                        operator: Operator {
                            kind: OperatorKind::Add,
                            location: cols(8, 8)
                        },
                        left: Expression::Int(Box::new(IntLiteral {
                            value: "2".to_string(),
                            location: cols(6, 6)
                        })),
                        right: Expression::Int(Box::new(IntLiteral {
                            value: "3".to_string(),
                            location: cols(10, 10)
                        })),
                        location: cols(6, 10)
                    })),
                    location: cols(5, 11)
                })),
                location: cols(1, 11)
            }))
        );
    }

    #[test]
    fn test_next_expression() {
        assert_eq!(
            expr("next"),
            Expression::Next(Box::new(Next { location: cols(1, 4) }))
        );
    }

    #[test]
    fn test_break_expression() {
        assert_eq!(
            expr("break"),
            Expression::Break(Box::new(Break { location: cols(1, 5) }))
        );
    }

    #[test]
    fn test_reference_expression() {
        assert_eq!(
            expr("ref 10"),
            Expression::Ref(Box::new(Ref {
                value: Expression::Int(Box::new(IntLiteral {
                    value: "10".to_string(),
                    location: cols(5, 6)
                })),
                location: cols(1, 6)
            }))
        );
    }

    #[test]
    fn test_mutable_reference_expression() {
        assert_eq!(
            expr("mut 10"),
            Expression::Mut(Box::new(Mut {
                value: Expression::Int(Box::new(IntLiteral {
                    value: "10".to_string(),
                    location: cols(5, 6)
                })),
                location: cols(1, 6)
            }))
        );
    }

    #[test]
    fn test_recover_expression() {
        assert_eq!(
            expr("recover 10"),
            Expression::Recover(Box::new(Recover {
                body: Expressions {
                    values: vec![Expression::Int(Box::new(IntLiteral {
                        value: "10".to_string(),
                        location: cols(9, 10)
                    }))],
                    location: cols(9, 10)
                },
                location: cols(1, 10)
            }))
        );

        assert_eq!(
            expr("recover { 10 }"),
            Expression::Recover(Box::new(Recover {
                body: Expressions {
                    values: vec![Expression::Int(Box::new(IntLiteral {
                        value: "10".to_string(),
                        location: cols(11, 12)
                    }))],
                    location: cols(9, 14)
                },
                location: cols(1, 14)
            }))
        );
    }

    #[test]
    fn test_condition_expression() {
        assert_eq!(
            expr("10 and 20"),
            Expression::And(Box::new(And {
                left: Expression::Int(Box::new(IntLiteral {
                    value: "10".to_string(),
                    location: cols(1, 2)
                })),
                right: Expression::Int(Box::new(IntLiteral {
                    value: "20".to_string(),
                    location: cols(8, 9)
                })),
                location: cols(1, 9)
            }))
        );

        assert_eq!(
            expr("10 + 2 and 20"),
            Expression::And(Box::new(And {
                left: Expression::Binary(Box::new(Binary {
                    operator: Operator {
                        kind: OperatorKind::Add,
                        location: cols(4, 4)
                    },
                    left: Expression::Int(Box::new(IntLiteral {
                        value: "10".to_string(),
                        location: cols(1, 2)
                    })),
                    right: Expression::Int(Box::new(IntLiteral {
                        value: "2".to_string(),
                        location: cols(6, 6)
                    })),
                    location: cols(1, 6)
                })),
                right: Expression::Int(Box::new(IntLiteral {
                    value: "20".to_string(),
                    location: cols(12, 13)
                })),
                location: cols(1, 13)
            }))
        );

        assert_eq!(
            expr("10 or 20"),
            Expression::Or(Box::new(Or {
                left: Expression::Int(Box::new(IntLiteral {
                    value: "10".to_string(),
                    location: cols(1, 2)
                })),
                right: Expression::Int(Box::new(IntLiteral {
                    value: "20".to_string(),
                    location: cols(7, 8)
                })),
                location: cols(1, 8)
            }))
        );

        assert_eq!(
            expr("10 + 2 or 20"),
            Expression::Or(Box::new(Or {
                left: Expression::Binary(Box::new(Binary {
                    operator: Operator {
                        kind: OperatorKind::Add,
                        location: cols(4, 4)
                    },
                    left: Expression::Int(Box::new(IntLiteral {
                        value: "10".to_string(),
                        location: cols(1, 2)
                    })),
                    right: Expression::Int(Box::new(IntLiteral {
                        value: "2".to_string(),
                        location: cols(6, 6)
                    })),
                    location: cols(1, 6)
                })),
                right: Expression::Int(Box::new(IntLiteral {
                    value: "20".to_string(),
                    location: cols(11, 12)
                })),
                location: cols(1, 12)
            }))
        );
    }

    #[test]
    fn test_type_cast_expression() {
        assert_eq!(
            expr("10 as B"),
            Expression::TypeCast(Box::new(TypeCast {
                value: Expression::Int(Box::new(IntLiteral {
                    value: "10".to_string(),
                    location: cols(1, 2)
                })),
                cast_to: Type::Named(Box::new(TypeName {
                    name: Constant {
                        source: None,
                        name: "B".to_string(),
                        location: cols(7, 7)
                    },
                    arguments: None,
                    location: cols(7, 7)
                })),
                location: cols(1, 7)
            }))
        );

        assert_eq!(
            expr("10 + 2 as B"),
            Expression::TypeCast(Box::new(TypeCast {
                value: Expression::Binary(Box::new(Binary {
                    operator: Operator {
                        kind: OperatorKind::Add,
                        location: cols(4, 4)
                    },
                    left: Expression::Int(Box::new(IntLiteral {
                        value: "10".to_string(),
                        location: cols(1, 2)
                    })),
                    right: Expression::Int(Box::new(IntLiteral {
                        value: "2".to_string(),
                        location: cols(6, 6)
                    })),
                    location: cols(1, 6)
                })),
                cast_to: Type::Named(Box::new(TypeName {
                    name: Constant {
                        source: None,
                        name: "B".to_string(),
                        location: cols(11, 11)
                    },
                    arguments: None,
                    location: cols(11, 11)
                })),
                location: cols(1, 11)
            }))
        );

        assert_eq!(
            expr("10 as B as C"),
            Expression::TypeCast(Box::new(TypeCast {
                value: Expression::TypeCast(Box::new(TypeCast {
                    value: Expression::Int(Box::new(IntLiteral {
                        value: "10".to_string(),
                        location: cols(1, 2)
                    })),
                    cast_to: Type::Named(Box::new(TypeName {
                        name: Constant {
                            source: None,
                            name: "B".to_string(),
                            location: cols(7, 7)
                        },
                        arguments: None,
                        location: cols(7, 7)
                    })),
                    location: cols(1, 7)
                })),
                cast_to: Type::Named(Box::new(TypeName {
                    name: Constant {
                        source: None,
                        name: "C".to_string(),
                        location: cols(12, 12)
                    },
                    arguments: None,
                    location: cols(12, 12)
                })),
                location: cols(1, 12)
            }))
        );
    }

    #[test]
    fn test_throw_expression() {
        assert_eq!(
            expr("throw 10"),
            Expression::Throw(Box::new(Throw {
                value: Expression::Int(Box::new(IntLiteral {
                    value: "10".to_string(),
                    location: cols(7, 8)
                })),
                location: cols(1, 8)
            }))
        );
    }

    #[test]
    fn test_return_expression() {
        assert_eq!(
            expr("return A"),
            Expression::Return(Box::new(Return {
                value: Some(Expression::Constant(Box::new(Constant {
                    source: None,
                    name: "A".to_string(),
                    location: cols(8, 8)
                }))),
                location: cols(1, 8)
            }))
        );

        assert_eq!(
            expr("return { 10 }"),
            Expression::Return(Box::new(Return {
                value: Some(Expression::Scope(Box::new(Scope {
                    body: Expressions {
                        values: vec![Expression::Int(Box::new(IntLiteral {
                            value: "10".to_string(),
                            location: cols(10, 11)
                        }))],
                        location: cols(8, 13)
                    },
                    location: cols(8, 13)
                }))),
                location: cols(1, 13)
            }))
        );

        assert_eq!(
            expr("return fn {}"),
            Expression::Return(Box::new(Return {
                value: Some(Expression::Closure(Box::new(Closure {
                    moving: false,
                    arguments: None,
                    return_type: None,
                    body: Expressions {
                        values: Vec::new(),
                        location: cols(11, 12)
                    },
                    location: cols(8, 12)
                }))),
                location: cols(1, 12)
            }))
        );

        assert_eq!(
            expr("return \"\""),
            Expression::Return(Box::new(Return {
                value: Some(Expression::String(Box::new(StringLiteral {
                    values: Vec::new(),
                    location: cols(8, 9)
                }))),
                location: cols(1, 9)
            }))
        );

        assert_eq!(
            expr("return ''"),
            Expression::Return(Box::new(Return {
                value: Some(Expression::String(Box::new(StringLiteral {
                    values: Vec::new(),
                    location: cols(8, 9)
                }))),
                location: cols(1, 9)
            }))
        );

        assert_eq!(
            expr("return @a"),
            Expression::Return(Box::new(Return {
                value: Some(Expression::Field(Box::new(Field {
                    name: "a".to_string(),
                    location: cols(8, 9)
                }))),
                location: cols(1, 9)
            }))
        );

        assert_eq!(
            expr("return a"),
            Expression::Return(Box::new(Return {
                value: Some(Expression::Identifier(Box::new(Identifier {
                    name: "a".to_string(),
                    location: cols(8, 8)
                }))),
                location: cols(1, 8)
            }))
        );

        assert_eq!(
            expr("return 10.0"),
            Expression::Return(Box::new(Return {
                value: Some(Expression::Float(Box::new(FloatLiteral {
                    value: "10.0".to_string(),
                    location: cols(8, 11)
                }))),
                location: cols(1, 11)
            }))
        );

        assert_eq!(
            expr("return 10"),
            Expression::Return(Box::new(Return {
                value: Some(Expression::Int(Box::new(IntLiteral {
                    value: "10".to_string(),
                    location: cols(8, 9)
                }))),
                location: cols(1, 9)
            }))
        );

        assert_eq!(
            expr("return self"),
            Expression::Return(Box::new(Return {
                value: Some(Expression::SelfObject(Box::new(SelfObject {
                    location: cols(8, 11)
                }))),
                location: cols(1, 11)
            }))
        );

        assert_eq!(
            expr("return (10)"),
            Expression::Return(Box::new(Return {
                value: Some(Expression::Group(Box::new(Group {
                    value: Expression::Int(Box::new(IntLiteral {
                        value: "10".to_string(),
                        location: cols(9, 10)
                    })),
                    location: cols(8, 11)
                }))),
                location: cols(1, 11)
            }))
        );

        assert_eq!(
            expr("return ref 10"),
            Expression::Return(Box::new(Return {
                value: Some(Expression::Ref(Box::new(Ref {
                    value: Expression::Int(Box::new(IntLiteral {
                        value: "10".to_string(),
                        location: cols(12, 13)
                    })),
                    location: cols(8, 13)
                }))),
                location: cols(1, 13)
            }))
        );

        assert_eq!(
            expr("return nil"),
            Expression::Return(Box::new(Return {
                value: Some(Expression::Nil(Box::new(Nil {
                    location: cols(8, 10)
                }))),
                location: cols(1, 10)
            }))
        );

        assert_eq!(
            expr("return true"),
            Expression::Return(Box::new(Return {
                value: Some(Expression::True(Box::new(True {
                    location: cols(8, 11)
                }))),
                location: cols(1, 11)
            }))
        );

        assert_eq!(
            expr("return false"),
            Expression::Return(Box::new(Return {
                value: Some(Expression::False(Box::new(False {
                    location: cols(8, 12)
                }))),
                location: cols(1, 12)
            }))
        );

        assert_eq!(
            expr("return recover {}"),
            Expression::Return(Box::new(Return {
                value: Some(Expression::Recover(Box::new(Recover {
                    body: Expressions {
                        values: Vec::new(),
                        location: cols(16, 17)
                    },
                    location: cols(8, 17)
                }))),
                location: cols(1, 17)
            }))
        );
    }

    #[test]
    fn test_return_expressions_with_newline() {
        let mut parser = parser("return\n10");
        let token1 = parser.require().unwrap();
        let node1 = parser.expression(token1).unwrap();
        let token2 = parser.require().unwrap();
        let node2 = parser.expression(token2).unwrap();

        assert_eq!(
            node1,
            Expression::Return(Box::new(Return {
                value: None,
                location: cols(1, 6)
            }))
        );

        assert_eq!(
            node2,
            Expression::Int(Box::new(IntLiteral {
                value: "10".to_string(),
                location: location(2..=2, 1..=2)
            }))
        );
    }

    #[test]
    fn test_try_expression() {
        assert_eq!(
            expr("try a"),
            Expression::Try(Box::new(Try {
                value: Expression::Identifier(Box::new(Identifier {
                    name: "a".to_string(),
                    location: cols(5, 5)
                })),
                location: cols(1, 5)
            }))
        );
    }

    #[test]
    fn test_if_expression() {
        assert_eq!(
            expr("if a { b }"),
            Expression::If(Box::new(If {
                if_true: IfCondition {
                    condition: Expression::Identifier(Box::new(Identifier {
                        name: "a".to_string(),
                        location: cols(4, 4)
                    })),
                    body: Expressions {
                        values: vec![Expression::Identifier(Box::new(
                            Identifier {
                                name: "b".to_string(),
                                location: cols(8, 8)
                            }
                        ))],
                        location: cols(6, 10)
                    },
                    location: cols(4, 10)
                },
                else_if: Vec::new(),
                else_body: None,
                location: cols(1, 10)
            }))
        );

        assert_eq!(
            expr("if A { b }"),
            Expression::If(Box::new(If {
                if_true: IfCondition {
                    condition: Expression::Constant(Box::new(Constant {
                        source: None,
                        name: "A".to_string(),
                        location: cols(4, 4)
                    })),
                    body: Expressions {
                        values: vec![Expression::Identifier(Box::new(
                            Identifier {
                                name: "b".to_string(),
                                location: cols(8, 8)
                            }
                        ))],
                        location: cols(6, 10)
                    },
                    location: cols(4, 10)
                },
                else_if: Vec::new(),
                else_body: None,
                location: cols(1, 10)
            }))
        );

        assert_eq!(
            expr("if a { b } else { c }"),
            Expression::If(Box::new(If {
                if_true: IfCondition {
                    condition: Expression::Identifier(Box::new(Identifier {
                        name: "a".to_string(),
                        location: cols(4, 4)
                    })),
                    body: Expressions {
                        values: vec![Expression::Identifier(Box::new(
                            Identifier {
                                name: "b".to_string(),
                                location: cols(8, 8)
                            }
                        ))],
                        location: cols(6, 10)
                    },
                    location: cols(4, 10)
                },
                else_if: Vec::new(),
                else_body: Some(Expressions {
                    values: vec![Expression::Identifier(Box::new(
                        Identifier {
                            name: "c".to_string(),
                            location: cols(19, 19)
                        }
                    ))],
                    location: cols(17, 21)
                }),
                location: cols(1, 21)
            }))
        );

        assert_eq!(
            expr("if a { b } else if c { d }"),
            Expression::If(Box::new(If {
                if_true: IfCondition {
                    condition: Expression::Identifier(Box::new(Identifier {
                        name: "a".to_string(),
                        location: cols(4, 4)
                    })),
                    body: Expressions {
                        values: vec![Expression::Identifier(Box::new(
                            Identifier {
                                name: "b".to_string(),
                                location: cols(8, 8)
                            }
                        ))],
                        location: cols(6, 10)
                    },
                    location: cols(4, 10)
                },
                else_if: vec![IfCondition {
                    condition: Expression::Identifier(Box::new(Identifier {
                        name: "c".to_string(),
                        location: cols(20, 20)
                    })),
                    body: Expressions {
                        values: vec![Expression::Identifier(Box::new(
                            Identifier {
                                name: "d".to_string(),
                                location: cols(24, 24)
                            }
                        ))],
                        location: cols(22, 26)
                    },
                    location: cols(20, 26)
                },],
                else_body: None,
                location: cols(1, 26)
            }))
        );

        assert_eq!(
            expr("if a { b } else if c { d } else if e { f }"),
            Expression::If(Box::new(If {
                if_true: IfCondition {
                    condition: Expression::Identifier(Box::new(Identifier {
                        name: "a".to_string(),
                        location: cols(4, 4)
                    })),
                    body: Expressions {
                        values: vec![Expression::Identifier(Box::new(
                            Identifier {
                                name: "b".to_string(),
                                location: cols(8, 8)
                            }
                        ))],
                        location: cols(6, 10)
                    },
                    location: cols(4, 10)
                },
                else_if: vec![
                    IfCondition {
                        condition: Expression::Identifier(Box::new(
                            Identifier {
                                name: "c".to_string(),
                                location: cols(20, 20)
                            }
                        )),
                        body: Expressions {
                            values: vec![Expression::Identifier(Box::new(
                                Identifier {
                                    name: "d".to_string(),
                                    location: cols(24, 24)
                                }
                            ))],
                            location: cols(22, 26)
                        },
                        location: cols(20, 26)
                    },
                    IfCondition {
                        condition: Expression::Identifier(Box::new(
                            Identifier {
                                name: "e".to_string(),
                                location: cols(36, 36)
                            }
                        )),
                        body: Expressions {
                            values: vec![Expression::Identifier(Box::new(
                                Identifier {
                                    name: "f".to_string(),
                                    location: cols(40, 40)
                                }
                            ))],
                            location: cols(38, 42)
                        },
                        location: cols(36, 42)
                    },
                ],
                else_body: None,
                location: cols(1, 42)
            }))
        );

        assert_eq!(
            expr("if a { b } else if c { d } else { e }"),
            Expression::If(Box::new(If {
                if_true: IfCondition {
                    condition: Expression::Identifier(Box::new(Identifier {
                        name: "a".to_string(),
                        location: cols(4, 4)
                    })),
                    body: Expressions {
                        values: vec![Expression::Identifier(Box::new(
                            Identifier {
                                name: "b".to_string(),
                                location: cols(8, 8)
                            }
                        ))],
                        location: cols(6, 10)
                    },
                    location: cols(4, 10)
                },
                else_if: vec![IfCondition {
                    condition: Expression::Identifier(Box::new(Identifier {
                        name: "c".to_string(),
                        location: cols(20, 20)
                    })),
                    body: Expressions {
                        values: vec![Expression::Identifier(Box::new(
                            Identifier {
                                name: "d".to_string(),
                                location: cols(24, 24)
                            }
                        ))],
                        location: cols(22, 26)
                    },
                    location: cols(20, 26)
                },],
                else_body: Some(Expressions {
                    values: vec![Expression::Identifier(Box::new(
                        Identifier {
                            name: "e".to_string(),
                            location: cols(35, 35)
                        }
                    ))],
                    location: cols(33, 37)
                }),
                location: cols(1, 37)
            }))
        );
    }

    #[test]
    fn test_invalid_if_expressions() {
        assert_error_expr!("if foo { b } else if", cols(20, 20));
        assert_error_expr!("if foo { b } else", cols(17, 17));
    }

    #[test]
    fn test_match_int_pattern() {
        assert_eq!(
            expr("match 1 { case 1 -> { 2 } }"),
            Expression::Match(Box::new(Match {
                expression: Expression::Int(Box::new(IntLiteral {
                    value: "1".to_string(),
                    location: cols(7, 7)
                })),
                expressions: vec![MatchExpression::Case(Box::new(MatchCase {
                    pattern: Pattern::Int(Box::new(IntLiteral {
                        value: "1".to_string(),
                        location: cols(16, 16)
                    })),
                    guard: None,
                    body: Expressions {
                        values: vec![Expression::Int(Box::new(IntLiteral {
                            value: "2".to_string(),
                            location: cols(23, 23)
                        }))],
                        location: cols(21, 25)
                    },
                    location: cols(11, 25)
                }))],
                location: cols(1, 27)
            }))
        );
    }

    #[test]
    fn test_match_or_pattern() {
        assert_eq!(
            expr("match 1 { case 1 or 2 -> { 2 } }"),
            Expression::Match(Box::new(Match {
                expression: Expression::Int(Box::new(IntLiteral {
                    value: "1".to_string(),
                    location: cols(7, 7)
                })),
                expressions: vec![MatchExpression::Case(Box::new(MatchCase {
                    pattern: Pattern::Or(Box::new(OrPattern {
                        patterns: vec![
                            Pattern::Int(Box::new(IntLiteral {
                                value: "1".to_string(),
                                location: cols(16, 16)
                            })),
                            Pattern::Int(Box::new(IntLiteral {
                                value: "2".to_string(),
                                location: cols(21, 21)
                            }))
                        ],
                        location: cols(16, 21)
                    })),
                    guard: None,
                    body: Expressions {
                        values: vec![Expression::Int(Box::new(IntLiteral {
                            value: "2".to_string(),
                            location: cols(28, 28)
                        }))],
                        location: cols(26, 30)
                    },
                    location: cols(11, 30)
                }))],
                location: cols(1, 32)
            }))
        );
    }

    #[test]
    fn test_match_boolean_pattern() {
        assert_eq!(
            expr("match 1 { case true -> { 2 } }"),
            Expression::Match(Box::new(Match {
                expression: Expression::Int(Box::new(IntLiteral {
                    value: "1".to_string(),
                    location: cols(7, 7)
                })),
                expressions: vec![MatchExpression::Case(Box::new(MatchCase {
                    pattern: Pattern::True(Box::new(True {
                        location: cols(16, 19)
                    })),
                    guard: None,
                    body: Expressions {
                        values: vec![Expression::Int(Box::new(IntLiteral {
                            value: "2".to_string(),
                            location: cols(26, 26)
                        }))],
                        location: cols(24, 28)
                    },
                    location: cols(11, 28)
                }))],
                location: cols(1, 30)
            }))
        );

        assert_eq!(
            expr("match 1 { case false -> { 2 } }"),
            Expression::Match(Box::new(Match {
                expression: Expression::Int(Box::new(IntLiteral {
                    value: "1".to_string(),
                    location: cols(7, 7)
                })),
                expressions: vec![MatchExpression::Case(Box::new(MatchCase {
                    pattern: Pattern::False(Box::new(False {
                        location: cols(16, 20)
                    })),
                    guard: None,
                    body: Expressions {
                        values: vec![Expression::Int(Box::new(IntLiteral {
                            value: "2".to_string(),
                            location: cols(27, 27)
                        }))],
                        location: cols(25, 29)
                    },
                    location: cols(11, 29)
                }))],
                location: cols(1, 31)
            }))
        );
    }

    #[test]
    fn test_match_single_string_pattern() {
        assert_eq!(
            expr("match 1 { case 'a' -> { 2 } }"),
            Expression::Match(Box::new(Match {
                expression: Expression::Int(Box::new(IntLiteral {
                    value: "1".to_string(),
                    location: cols(7, 7)
                })),
                expressions: vec![MatchExpression::Case(Box::new(MatchCase {
                    pattern: Pattern::String(Box::new(StringLiteral {
                        values: vec![StringValue::Text(Box::new(StringText {
                            value: "a".to_string(),
                            location: cols(17, 17)
                        }))],
                        location: cols(16, 18)
                    })),
                    guard: None,
                    body: Expressions {
                        values: vec![Expression::Int(Box::new(IntLiteral {
                            value: "2".to_string(),
                            location: cols(25, 25)
                        }))],
                        location: cols(23, 27)
                    },
                    location: cols(11, 27)
                }))],
                location: cols(1, 29)
            }))
        );
    }

    #[test]
    fn test_match_empty_single_string_pattern() {
        assert_eq!(
            expr("match 1 { case '' -> { 2 } }"),
            Expression::Match(Box::new(Match {
                expression: Expression::Int(Box::new(IntLiteral {
                    value: "1".to_string(),
                    location: cols(7, 7)
                })),
                expressions: vec![MatchExpression::Case(Box::new(MatchCase {
                    pattern: Pattern::String(Box::new(StringLiteral {
                        values: Vec::new(),
                        location: cols(16, 17)
                    })),
                    guard: None,
                    body: Expressions {
                        values: vec![Expression::Int(Box::new(IntLiteral {
                            value: "2".to_string(),
                            location: cols(24, 24)
                        }))],
                        location: cols(22, 26)
                    },
                    location: cols(11, 26)
                }))],
                location: cols(1, 28)
            }))
        );
    }

    #[test]
    fn test_match_double_string_pattern() {
        assert_eq!(
            expr("match 1 { case \"a\" -> { 2 } }"),
            Expression::Match(Box::new(Match {
                expression: Expression::Int(Box::new(IntLiteral {
                    value: "1".to_string(),
                    location: cols(7, 7)
                })),
                expressions: vec![MatchExpression::Case(Box::new(MatchCase {
                    pattern: Pattern::String(Box::new(StringLiteral {
                        values: vec![StringValue::Text(Box::new(StringText {
                            value: "a".to_string(),
                            location: cols(17, 17)
                        }))],
                        location: cols(16, 18)
                    })),
                    guard: None,
                    body: Expressions {
                        values: vec![Expression::Int(Box::new(IntLiteral {
                            value: "2".to_string(),
                            location: cols(25, 25)
                        }))],
                        location: cols(23, 27)
                    },
                    location: cols(11, 27)
                }))],
                location: cols(1, 29)
            }))
        );
    }

    #[test]
    fn test_match_empty_double_string_pattern() {
        assert_eq!(
            expr("match 1 { case \"\" -> { 2 } }"),
            Expression::Match(Box::new(Match {
                expression: Expression::Int(Box::new(IntLiteral {
                    value: "1".to_string(),
                    location: cols(7, 7)
                })),
                expressions: vec![MatchExpression::Case(Box::new(MatchCase {
                    pattern: Pattern::String(Box::new(StringLiteral {
                        values: Vec::new(),
                        location: cols(16, 17)
                    })),
                    guard: None,
                    body: Expressions {
                        values: vec![Expression::Int(Box::new(IntLiteral {
                            value: "2".to_string(),
                            location: cols(24, 24)
                        }))],
                        location: cols(22, 26)
                    },
                    location: cols(11, 26)
                }))],
                location: cols(1, 28)
            }))
        );
    }

    #[test]
    fn test_match_constant_pattern() {
        assert_eq!(
            expr("match 1 { case A -> { 2 } }"),
            Expression::Match(Box::new(Match {
                expression: Expression::Int(Box::new(IntLiteral {
                    value: "1".to_string(),
                    location: cols(7, 7)
                })),
                expressions: vec![MatchExpression::Case(Box::new(MatchCase {
                    pattern: Pattern::Constant(Box::new(Constant {
                        source: None,
                        name: "A".to_string(),
                        location: cols(16, 16)
                    })),
                    guard: None,
                    body: Expressions {
                        values: vec![Expression::Int(Box::new(IntLiteral {
                            value: "2".to_string(),
                            location: cols(23, 23)
                        }))],
                        location: cols(21, 25)
                    },
                    location: cols(11, 25)
                }))],
                location: cols(1, 27)
            }))
        );

        assert_eq!(
            expr("match A {}"),
            Expression::Match(Box::new(Match {
                expression: Expression::Constant(Box::new(Constant {
                    source: None,
                    name: "A".to_string(),
                    location: cols(7, 7)
                })),
                expressions: Vec::new(),
                location: cols(1, 10)
            }))
        );
    }

    #[test]
    fn test_match_identifier_pattern() {
        assert_eq!(
            expr("match 1 { case a -> { 2 } }"),
            Expression::Match(Box::new(Match {
                expression: Expression::Int(Box::new(IntLiteral {
                    value: "1".to_string(),
                    location: cols(7, 7)
                })),
                expressions: vec![MatchExpression::Case(Box::new(MatchCase {
                    pattern: Pattern::Identifier(Box::new(IdentifierPattern {
                        name: Identifier {
                            name: "a".to_string(),
                            location: cols(16, 16)
                        },
                        mutable: false,
                        value_type: None,
                        location: cols(16, 16)
                    })),
                    guard: None,
                    body: Expressions {
                        values: vec![Expression::Int(Box::new(IntLiteral {
                            value: "2".to_string(),
                            location: cols(23, 23)
                        }))],
                        location: cols(21, 25)
                    },
                    location: cols(11, 25)
                }))],
                location: cols(1, 27)
            }))
        );

        assert_eq!(
            expr("match 1 { case mut a -> { 2 } }"),
            Expression::Match(Box::new(Match {
                expression: Expression::Int(Box::new(IntLiteral {
                    value: "1".to_string(),
                    location: cols(7, 7)
                })),
                expressions: vec![MatchExpression::Case(Box::new(MatchCase {
                    pattern: Pattern::Identifier(Box::new(IdentifierPattern {
                        name: Identifier {
                            name: "a".to_string(),
                            location: cols(20, 20)
                        },
                        mutable: true,
                        value_type: None,
                        location: cols(16, 20)
                    })),
                    guard: None,
                    body: Expressions {
                        values: vec![Expression::Int(Box::new(IntLiteral {
                            value: "2".to_string(),
                            location: cols(27, 27)
                        }))],
                        location: cols(25, 29)
                    },
                    location: cols(11, 29)
                }))],
                location: cols(1, 31)
            }))
        );
    }

    #[test]
    fn test_match_wildcard_pattern() {
        assert_eq!(
            expr("match 1 { case _ -> { 2 } }"),
            Expression::Match(Box::new(Match {
                expression: Expression::Int(Box::new(IntLiteral {
                    value: "1".to_string(),
                    location: cols(7, 7)
                })),
                expressions: vec![MatchExpression::Case(Box::new(MatchCase {
                    pattern: Pattern::Wildcard(Box::new(WildcardPattern {
                        location: cols(16, 16)
                    })),
                    guard: None,
                    body: Expressions {
                        values: vec![Expression::Int(Box::new(IntLiteral {
                            value: "2".to_string(),
                            location: cols(23, 23)
                        }))],
                        location: cols(21, 25)
                    },
                    location: cols(11, 25)
                }))],
                location: cols(1, 27)
            }))
        );
    }

    #[test]
    fn test_match_namespaced_constant_pattern() {
        assert_eq!(
            expr("match 1 { case a.B -> { 2 } }"),
            Expression::Match(Box::new(Match {
                expression: Expression::Int(Box::new(IntLiteral {
                    value: "1".to_string(),
                    location: cols(7, 7)
                })),
                expressions: vec![MatchExpression::Case(Box::new(MatchCase {
                    pattern: Pattern::Constant(Box::new(Constant {
                        source: Some(Identifier {
                            name: "a".to_string(),
                            location: cols(16, 16)
                        }),
                        name: "B".to_string(),
                        location: cols(16, 18)
                    })),
                    guard: None,
                    body: Expressions {
                        values: vec![Expression::Int(Box::new(IntLiteral {
                            value: "2".to_string(),
                            location: cols(25, 25)
                        }))],
                        location: cols(23, 27)
                    },
                    location: cols(11, 27)
                }))],
                location: cols(1, 29)
            }))
        );
    }

    #[test]
    fn test_match_destructure_pattern() {
        assert_eq!(
            expr("match 1 { case A(1) -> { 2 } }"),
            Expression::Match(Box::new(Match {
                expression: Expression::Int(Box::new(IntLiteral {
                    value: "1".to_string(),
                    location: cols(7, 7)
                })),
                expressions: vec![MatchExpression::Case(Box::new(MatchCase {
                    pattern: Pattern::Variant(Box::new(VariantPattern {
                        name: Constant {
                            source: None,
                            name: "A".to_string(),
                            location: cols(16, 16)
                        },
                        values: vec![Pattern::Int(Box::new(IntLiteral {
                            value: "1".to_string(),
                            location: cols(18, 18)
                        }))],
                        location: cols(16, 19)
                    })),
                    guard: None,
                    body: Expressions {
                        values: vec![Expression::Int(Box::new(IntLiteral {
                            value: "2".to_string(),
                            location: cols(26, 26)
                        }))],
                        location: cols(24, 28)
                    },
                    location: cols(11, 28)
                }))],
                location: cols(1, 30)
            }))
        );
    }

    #[test]
    fn test_match_tuple_pattern() {
        assert_eq!(
            expr("match 1 { case (1, 2) -> { 2 } }"),
            Expression::Match(Box::new(Match {
                expression: Expression::Int(Box::new(IntLiteral {
                    value: "1".to_string(),
                    location: cols(7, 7)
                })),
                expressions: vec![MatchExpression::Case(Box::new(MatchCase {
                    pattern: Pattern::Tuple(Box::new(TuplePattern {
                        values: vec![
                            Pattern::Int(Box::new(IntLiteral {
                                value: "1".to_string(),
                                location: cols(17, 17)
                            })),
                            Pattern::Int(Box::new(IntLiteral {
                                value: "2".to_string(),
                                location: cols(20, 20)
                            })),
                        ],
                        location: cols(16, 21)
                    })),
                    guard: None,
                    body: Expressions {
                        values: vec![Expression::Int(Box::new(IntLiteral {
                            value: "2".to_string(),
                            location: cols(28, 28)
                        }))],
                        location: cols(26, 30)
                    },
                    location: cols(11, 30)
                }))],
                location: cols(1, 32)
            }))
        );

        assert_eq!(
            expr("match 1 { case (1, 2,) -> { 2 } }"),
            Expression::Match(Box::new(Match {
                expression: Expression::Int(Box::new(IntLiteral {
                    value: "1".to_string(),
                    location: cols(7, 7)
                })),
                expressions: vec![MatchExpression::Case(Box::new(MatchCase {
                    pattern: Pattern::Tuple(Box::new(TuplePattern {
                        values: vec![
                            Pattern::Int(Box::new(IntLiteral {
                                value: "1".to_string(),
                                location: cols(17, 17)
                            })),
                            Pattern::Int(Box::new(IntLiteral {
                                value: "2".to_string(),
                                location: cols(20, 20)
                            })),
                        ],
                        location: cols(16, 22)
                    })),
                    guard: None,
                    body: Expressions {
                        values: vec![Expression::Int(Box::new(IntLiteral {
                            value: "2".to_string(),
                            location: cols(29, 29)
                        }))],
                        location: cols(27, 31)
                    },
                    location: cols(11, 31)
                }))],
                location: cols(1, 33)
            }))
        );
    }

    #[test]
    fn test_match_empty_class_pattern() {
        assert_eq!(
            expr("match 1 { case {} -> {} }"),
            Expression::Match(Box::new(Match {
                expression: Expression::Int(Box::new(IntLiteral {
                    value: "1".to_string(),
                    location: cols(7, 7)
                })),
                expressions: vec![MatchExpression::Case(Box::new(MatchCase {
                    pattern: Pattern::Class(Box::new(ClassPattern {
                        values: Vec::new(),
                        location: cols(16, 17)
                    })),
                    guard: None,
                    body: Expressions {
                        values: Vec::new(),
                        location: cols(22, 23)
                    },
                    location: cols(11, 23)
                }))],
                location: cols(1, 25)
            }))
        );
    }

    #[test]
    fn test_match_class_pattern() {
        assert_eq!(
            expr("match 1 { case { @a = _ } -> {} }"),
            Expression::Match(Box::new(Match {
                expression: Expression::Int(Box::new(IntLiteral {
                    value: "1".to_string(),
                    location: cols(7, 7)
                })),
                expressions: vec![MatchExpression::Case(Box::new(MatchCase {
                    pattern: Pattern::Class(Box::new(ClassPattern {
                        values: vec![FieldPattern {
                            field: Field {
                                name: "a".to_string(),
                                location: cols(18, 19)
                            },
                            pattern: Pattern::Wildcard(Box::new(
                                WildcardPattern { location: cols(23, 23) }
                            )),
                            location: cols(18, 23)
                        }],
                        location: cols(16, 25)
                    })),
                    guard: None,
                    body: Expressions {
                        values: Vec::new(),
                        location: cols(30, 31)
                    },
                    location: cols(11, 31)
                }))],
                location: cols(1, 33)
            }))
        );
    }

    #[test]
    fn test_match_without_curly_braces() {
        assert_eq!(
            expr("match 1 { case 1 -> 2 }"),
            Expression::Match(Box::new(Match {
                expression: Expression::Int(Box::new(IntLiteral {
                    value: "1".to_string(),
                    location: cols(7, 7)
                })),
                expressions: vec![MatchExpression::Case(Box::new(MatchCase {
                    pattern: Pattern::Int(Box::new(IntLiteral {
                        value: "1".to_string(),
                        location: cols(16, 16)
                    })),
                    guard: None,
                    body: Expressions {
                        values: vec![Expression::Int(Box::new(IntLiteral {
                            value: "2".to_string(),
                            location: cols(21, 21)
                        }))],
                        location: cols(21, 21)
                    },
                    location: cols(11, 21)
                }))],
                location: cols(1, 23)
            }))
        );
    }

    #[test]
    fn test_loop_expression() {
        assert_eq!(
            expr("loop { 10 }"),
            Expression::Loop(Box::new(Loop {
                body: Expressions {
                    values: vec![Expression::Int(Box::new(IntLiteral {
                        value: "10".to_string(),
                        location: cols(8, 9)
                    }))],
                    location: cols(6, 11)
                },
                location: cols(1, 11)
            }))
        );
    }

    #[test]
    fn test_invalid_loop_expression() {
        assert_error_expr!("loop 10 }", cols(6, 7));
    }

    #[test]
    fn test_while_expression() {
        assert_eq!(
            expr("while 10 { 20 }"),
            Expression::While(Box::new(While {
                condition: Expression::Int(Box::new(IntLiteral {
                    value: "10".to_string(),
                    location: cols(7, 8)
                })),
                body: Expressions {
                    values: vec![Expression::Int(Box::new(IntLiteral {
                        value: "20".to_string(),
                        location: cols(12, 13)
                    }))],
                    location: cols(10, 15)
                },
                location: cols(1, 15)
            }))
        );
    }

    #[test]
    fn test_invalid_while_expression() {
        assert_error_expr!("while 10 20 }", cols(10, 11));
    }

    #[test]
    fn test_enum_class() {
        assert_eq!(
            top(parse("class enum Option[T] { case Some(T) case None }")),
            TopLevelExpression::DefineClass(Box::new(DefineClass {
                public: false,
                kind: ClassKind::Enum,
                name: Constant {
                    source: None,
                    name: "Option".to_string(),
                    location: cols(12, 17),
                },
                type_parameters: Some(TypeParameters {
                    values: vec![TypeParameter {
                        name: Constant {
                            source: None,
                            name: "T".to_string(),
                            location: cols(19, 19)
                        },
                        requirements: None,
                        location: cols(19, 19)
                    }],
                    location: cols(18, 20)
                }),
                body: ClassExpressions {
                    values: vec![
                        ClassExpression::DefineVariant(Box::new(
                            DefineVariant {
                                name: Constant {
                                    source: None,
                                    name: "Some".to_string(),
                                    location: cols(29, 32)
                                },
                                members: Some(Types {
                                    values: vec![Type::Named(Box::new(
                                        TypeName {
                                            name: Constant {
                                                source: None,
                                                name: "T".to_string(),
                                                location: location(
                                                    1..=1,
                                                    34..=34
                                                )
                                            },
                                            arguments: None,
                                            location: cols(34, 34)
                                        }
                                    ))],
                                    location: cols(33, 35)
                                }),
                                location: cols(24, 35)
                            },
                        )),
                        ClassExpression::DefineVariant(Box::new(
                            DefineVariant {
                                name: Constant {
                                    source: None,
                                    name: "None".to_string(),
                                    location: cols(42, 45)
                                },
                                members: None,
                                location: cols(37, 40)
                            },
                        ))
                    ],
                    location: cols(22, 47)
                },
                location: cols(1, 47)
            }))
        );
    }

    #[test]
    fn test_namespaced_constant() {
        assert_eq!(
            expr("a.B"),
            Expression::Call(Box::new(Call {
                receiver: Some(Expression::Identifier(Box::new(Identifier {
                    name: "a".to_string(),
                    location: cols(1, 1)
                }))),
                name: Identifier {
                    name: "B".to_string(),
                    location: cols(3, 3)
                },
                arguments: None,
                location: cols(1, 3)
            }))
        );
    }

    #[test]
    fn test_comments() {
        assert_eq!(
            top(parse_with_comments("# foo")),
            TopLevelExpression::Comment(Box::new(Comment {
                value: "foo".to_string(),
                location: cols(1, 5)
            }))
        );

        assert_eq!(
            top(parse_with_comments("class A {\n# foo\n}")),
            TopLevelExpression::DefineClass(Box::new(DefineClass {
                public: false,
                kind: ClassKind::Regular,
                name: Constant {
                    source: None,
                    name: "A".to_string(),
                    location: cols(7, 7)
                },
                type_parameters: None,
                body: ClassExpressions {
                    values: vec![ClassExpression::Comment(Box::new(Comment {
                        value: "foo".to_string(),
                        location: location(2..=2, 1..=5)
                    }))],
                    location: location(1..=3, 9..=1)
                },
                location: location(1..=3, 1..=1)
            }))
        );

        assert_eq!(
            top(parse_with_comments("trait A {\n# foo\n}")),
            TopLevelExpression::DefineTrait(Box::new(DefineTrait {
                public: false,
                name: Constant {
                    source: None,
                    name: "A".to_string(),
                    location: cols(7, 7)
                },
                type_parameters: None,
                requirements: None,
                body: TraitExpressions {
                    values: vec![TraitExpression::Comment(Box::new(Comment {
                        value: "foo".to_string(),
                        location: location(2..=2, 1..=5)
                    }))],
                    location: location(1..=3, 9..=1)
                },
                location: location(1..=3, 1..=1)
            }))
        );

        assert_eq!(
            top(parse_with_comments("impl A {\n# foo\n}")),
            TopLevelExpression::ReopenClass(Box::new(ReopenClass {
                bounds: None,
                class_name: Constant {
                    source: None,
                    name: "A".to_string(),
                    location: cols(6, 6)
                },
                body: ImplementationExpressions {
                    values: vec![ImplementationExpression::Comment(Box::new(
                        Comment {
                            value: "foo".to_string(),
                            location: location(2..=2, 1..=5)
                        }
                    ))],
                    location: location(1..=3, 8..=1)
                },
                location: location(1..=3, 1..=1)
            }))
        );

        assert_eq!(
            top(parse_with_comments("impl A for B {\n# foo\n}")),
            TopLevelExpression::ImplementTrait(Box::new(ImplementTrait {
                bounds: None,
                trait_name: TypeName {
                    name: Constant {
                        source: None,
                        name: "A".to_string(),
                        location: cols(6, 6)
                    },
                    arguments: None,
                    location: cols(6, 6)
                },
                class_name: Constant {
                    source: None,
                    name: "B".to_string(),
                    location: cols(12, 12)
                },
                body: ImplementationExpressions {
                    values: vec![ImplementationExpression::Comment(Box::new(
                        Comment {
                            value: "foo".to_string(),
                            location: location(2..=2, 1..=5)
                        }
                    ))],
                    location: location(1..=3, 14..=1)
                },
                location: location(1..=3, 1..=1)
            }))
        );

        assert_eq!(
            expr_with_comments("# foo"),
            Expression::Comment(Box::new(Comment {
                value: "foo".to_string(),
                location: cols(1, 5)
            }))
        );
    }
}
