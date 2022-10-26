//! Types and methods for producing compiler diagnostics.
use ast::source_location::SourceLocation;
use std::fmt;
use std::path::PathBuf;

/// The unique ID of a diagnostic.
#[derive(PartialEq, Eq, Copy, Clone)]
pub(crate) enum DiagnosticId {
    DuplicateSymbol,
    InvalidAssign,
    InvalidBound,
    InvalidCall,
    InvalidClass,
    InvalidConstExpr,
    InvalidFile,
    InvalidImplementation,
    InvalidMethod,
    InvalidSyntax,
    InvalidTry,
    InvalidType,
    MissingTrait,
    PrivateSymbol,
    InvalidSymbol,
    InvalidLoopKeyword,
    InvalidThrow,
    MissingField,
    InvalidRef,
    InvalidPattern,
    InvalidField,
    MissingThrow,
    Unreachable,
    MovedVariable,
    InvalidMove,
    InvalidMatch,
    LimitReached,
    MissingMain,
}

impl fmt::Display for DiagnosticId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let id = match self {
            DiagnosticId::InvalidFile => "invalid-file",
            DiagnosticId::InvalidSyntax => "invalid-syntax",
            DiagnosticId::InvalidConstExpr => "invalid-const-expr",
            DiagnosticId::InvalidCall => "invalid-call",
            DiagnosticId::DuplicateSymbol => "duplicate-symbol",
            DiagnosticId::InvalidSymbol => "invalid-symbol",
            DiagnosticId::InvalidType => "invalid-type",
            DiagnosticId::MissingTrait => "missing-trait",
            DiagnosticId::InvalidBound => "invalid-bound",
            DiagnosticId::InvalidMethod => "invalid-method",
            DiagnosticId::InvalidImplementation => "invalid-implementation",
            DiagnosticId::InvalidClass => "invalid-class",
            DiagnosticId::PrivateSymbol => "private-symbol",
            DiagnosticId::InvalidTry => "invalid-try",
            DiagnosticId::InvalidAssign => "invalid-assign",
            DiagnosticId::InvalidLoopKeyword => "invalid-loop-keyword",
            DiagnosticId::InvalidThrow => "invalid-throw",
            DiagnosticId::MissingField => "missing-field",
            DiagnosticId::InvalidRef => "invalid-ref",
            DiagnosticId::InvalidPattern => "invalid-pattern",
            DiagnosticId::InvalidField => "invalid-field",
            DiagnosticId::MissingThrow => "missing-throw",
            DiagnosticId::Unreachable => "unreachable",
            DiagnosticId::MovedVariable => "moved-variable",
            DiagnosticId::InvalidMove => "invalid-move",
            DiagnosticId::InvalidMatch => "invalid-match",
            DiagnosticId::LimitReached => "limit-reached",
            DiagnosticId::MissingMain => "missing-main",
        };

        write!(f, "{}", id)
    }
}

impl fmt::Debug for DiagnosticId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self)
    }
}

/// The type/kind of a diagnostic.
#[derive(Copy, Clone)]
pub(crate) enum DiagnosticType {
    Warning,
    Error,
}

impl fmt::Display for DiagnosticType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            DiagnosticType::Warning => write!(f, "warning"),
            DiagnosticType::Error => write!(f, "error"),
        }
    }
}

/// A single diagnostic such as a warning or error.
pub(crate) struct Diagnostic {
    kind: DiagnosticType,
    id: DiagnosticId,
    message: String,
    file: PathBuf,
    location: SourceLocation,
}

impl Diagnostic {
    pub(crate) fn new(
        kind: DiagnosticType,
        id: DiagnosticId,
        message: String,
        file: PathBuf,
        location: SourceLocation,
    ) -> Self {
        Self { kind, id, message, file, location }
    }

    pub(crate) fn is_error(&self) -> bool {
        matches!(self.kind, DiagnosticType::Error)
    }

    pub(crate) fn kind(&self) -> DiagnosticType {
        self.kind
    }

    pub(crate) fn id(&self) -> DiagnosticId {
        self.id
    }

    pub(crate) fn message(&self) -> &String {
        &self.message
    }

    pub(crate) fn file(&self) -> &PathBuf {
        &self.file
    }

    pub(crate) fn location(&self) -> &SourceLocation {
        &self.location
    }
}

/// A collection of warnings and errors.
pub(crate) struct Diagnostics {
    values: Vec<Diagnostic>,

    /// A flag indicating one or more errors have been produced.
    ///
    /// We use a dedicated flag as checking for the presence of errors happens
    /// frequently. This avoids the need for iterating the diagnostics for every
    /// such check.
    errors: bool,
}

impl Diagnostics {
    pub(crate) fn new() -> Self {
        Self { values: Vec::new(), errors: false }
    }

    pub(crate) fn has_errors(&self) -> bool {
        self.errors
    }

    pub(crate) fn warn<S: Into<String>>(
        &mut self,
        id: DiagnosticId,
        message: S,
        file: PathBuf,
        location: SourceLocation,
    ) {
        self.values.push(Diagnostic::new(
            DiagnosticType::Warning,
            id,
            message.into(),
            file,
            location,
        ));
    }

    pub(crate) fn error<S: Into<String>>(
        &mut self,
        id: DiagnosticId,
        message: S,
        file: PathBuf,
        location: SourceLocation,
    ) {
        self.errors = true;

        self.values.push(Diagnostic::new(
            DiagnosticType::Error,
            id,
            message.into(),
            file,
            location,
        ));
    }

    pub(crate) fn undefined_symbol(
        &mut self,
        name: &str,
        file: PathBuf,
        location: SourceLocation,
    ) {
        self.error(
            DiagnosticId::InvalidSymbol,
            format!("The symbol '{}' is undefined", name),
            file,
            location,
        );
    }

    pub(crate) fn undefined_field(
        &mut self,
        name: &str,
        file: PathBuf,
        location: SourceLocation,
    ) {
        self.error(
            DiagnosticId::InvalidSymbol,
            format!("The field '{}' is undefined", name),
            file,
            location,
        );
    }

    pub(crate) fn duplicate_symbol(
        &mut self,
        name: &String,
        file: PathBuf,
        location: SourceLocation,
    ) {
        self.error(
            DiagnosticId::DuplicateSymbol,
            format!("The symbol '{}' is already defined", name),
            file,
            location,
        );
    }

    pub(crate) fn duplicate_type_parameter(
        &mut self,
        name: &str,
        file: PathBuf,
        location: SourceLocation,
    ) {
        self.error(
            DiagnosticId::DuplicateSymbol,
            format!("The type parameter '{}' is already defined", name),
            file,
            location,
        );
    }

    pub(crate) fn not_a_class(
        &mut self,
        name: &String,
        file: PathBuf,
        location: SourceLocation,
    ) {
        self.error(
            DiagnosticId::InvalidType,
            format!("'{}' isn't a class", name),
            file,
            location,
        );
    }

    pub(crate) fn duplicate_method(
        &mut self,
        method_name: &String,
        type_name: String,
        file: PathBuf,
        location: SourceLocation,
    ) {
        self.error(
            DiagnosticId::DuplicateSymbol,
            format!(
                "The method '{}' is already defined for type '{}'",
                method_name, type_name
            ),
            file,
            location,
        );
    }

    pub(crate) fn private_method_call(
        &mut self,
        name: &str,
        file: PathBuf,
        location: SourceLocation,
    ) {
        self.error(
            DiagnosticId::InvalidCall,
            format!("The method '{}' exists but is private", name),
            file,
            location,
        );
    }

    pub(crate) fn private_field(
        &mut self,
        name: &str,
        file: PathBuf,
        location: SourceLocation,
    ) {
        self.error(
            DiagnosticId::PrivateSymbol,
            format!("The field '{}' is private", name),
            file,
            location,
        );
    }

    pub(crate) fn type_error(
        &mut self,
        given: String,
        expected: String,
        file: PathBuf,
        location: SourceLocation,
    ) {
        self.error(
            DiagnosticId::InvalidType,
            format!(
                "Incorrect type: expected '{}', found '{}'",
                expected, given
            ),
            file,
            location,
        );
    }

    pub(crate) fn pattern_type_error(
        &mut self,
        given: String,
        expected: String,
        file: PathBuf,
        location: SourceLocation,
    ) {
        self.error(
            DiagnosticId::InvalidType,
            format!(
                "This pattern expects a value of type '{}', \
                but the value's type is '{}'",
                expected, given
            ),
            file,
            location,
        );
    }

    pub(crate) fn undefined_method(
        &mut self,
        name: &str,
        receiver: String,
        file: PathBuf,
        location: SourceLocation,
    ) {
        self.error(
            DiagnosticId::InvalidSymbol,
            format!(
                "The method '{}' isn't defined for type '{}'",
                name, receiver
            ),
            file,
            location,
        );
    }

    pub(crate) fn invalid_builtin_function(
        &mut self,
        file: PathBuf,
        location: SourceLocation,
    ) {
        self.error(
            DiagnosticId::InvalidCall,
            "Builtin functions can only be used in the standard library",
            file,
            location,
        );
    }

    pub(crate) fn tuple_size_error(
        &mut self,
        file: PathBuf,
        location: SourceLocation,
    ) {
        self.error(
            DiagnosticId::InvalidType,
            "Tuples are limited to up to 8 members",
            file,
            location,
        );
    }

    pub(crate) fn throw_not_allowed(
        &mut self,
        file: PathBuf,
        location: SourceLocation,
    ) {
        self.error(
            DiagnosticId::InvalidThrow,
            "Throwing isn't allowed, as the surrounding closure or method \
            doesn't specify a throw type",
            file,
            location,
        );
    }

    pub(crate) fn incorrect_call_arguments(
        &mut self,
        given: usize,
        expected: usize,
        file: PathBuf,
        location: SourceLocation,
    ) {
        self.error(
            DiagnosticId::InvalidCall,
            format!(
                "Incorrect number of arguments: expected {}, found {}",
                expected, given
            ),
            file,
            location,
        );
    }

    pub(crate) fn closure_with_named_argument(
        &mut self,
        file: PathBuf,
        location: SourceLocation,
    ) {
        self.error(
            DiagnosticId::InvalidCall,
            "Closures don't support named arguments",
            file,
            location,
        );
    }

    pub(crate) fn incorrect_pattern_arguments(
        &mut self,
        given: usize,
        expected: usize,
        file: PathBuf,
        location: SourceLocation,
    ) {
        self.error(
            DiagnosticId::InvalidPattern,
            format!(
                "Incorrect number of pattern arguments: expected {}, found {}",
                expected, given
            ),
            file,
            location,
        );
    }

    pub(crate) fn undefined_variant(
        &mut self,
        name: &str,
        type_name: String,
        file: PathBuf,
        location: SourceLocation,
    ) {
        self.error(
            DiagnosticId::InvalidSymbol,
            format!(
                "The variant '{}' doesn't exist for type '{}'",
                name, type_name
            ),
            file,
            location,
        );
    }

    pub(crate) fn symbol_not_a_module(
        &mut self,
        name: &str,
        file: PathBuf,
        location: SourceLocation,
    ) {
        self.error(
            DiagnosticId::InvalidSymbol,
            format!("The symbol '{}' isn't a module", name),
            file,
            location,
        );
    }

    pub(crate) fn symbol_not_a_value(
        &mut self,
        name: &str,
        file: PathBuf,
        location: SourceLocation,
    ) {
        self.error(
            DiagnosticId::InvalidSymbol,
            format!("The symbol '{}' is defined but isn't a value", name),
            file,
            location,
        )
    }

    pub(crate) fn invalid_instance_call(
        &mut self,
        name: &str,
        receiver: String,
        file: PathBuf,
        location: SourceLocation,
    ) {
        self.error(
            DiagnosticId::InvalidCall,
            format!(
                "The method '{}' exists for type '{}', \
                but is an instance method",
                name, receiver,
            ),
            file,
            location,
        );
    }

    pub(crate) fn invalid_static_call(
        &mut self,
        name: &str,
        receiver: String,
        file: PathBuf,
        location: SourceLocation,
    ) {
        self.error(
            DiagnosticId::InvalidCall,
            format!(
                "The method '{}' exists for type '{}', \
                but is a static method",
                name, receiver,
            ),
            file,
            location,
        );
    }

    pub(crate) fn never_throws(
        &mut self,
        file: PathBuf,
        location: SourceLocation,
    ) {
        self.error(
            DiagnosticId::InvalidTry,
            "This expression never throws",
            file,
            location,
        );
    }

    pub(crate) fn missing_throw(
        &mut self,
        name: String,
        file: PathBuf,
        location: SourceLocation,
    ) {
        self.error(
            DiagnosticId::MissingThrow,
            format!(
                "A value of type '{}' is expected to be thrown, \
                but no value is ever thrown",
                name
            ),
            file,
            location,
        );
    }

    pub(crate) fn unreachable(
        &mut self,
        file: PathBuf,
        location: SourceLocation,
    ) {
        self.warn(
            DiagnosticId::Unreachable,
            "This code is unreachable",
            file,
            location,
        );
    }

    pub(crate) fn unsendable_field(
        &mut self,
        field_name: &str,
        type_name: String,
        file: PathBuf,
        location: SourceLocation,
    ) {
        self.error(
            DiagnosticId::InvalidSymbol,
            format!(
                "The field '{}' can't be read as its type ('{}') \
                isn't sendable",
                field_name, type_name
            ),
            file,
            location,
        );
    }

    pub(crate) fn unsendable_type(
        &mut self,
        name: String,
        file: PathBuf,
        location: SourceLocation,
    ) {
        self.error(
            DiagnosticId::InvalidType,
            format!(
                "Values of type '{}' can't be sent between processes",
                name
            ),
            file,
            location,
        );
    }

    pub(crate) fn unsendable_type_in_recover(
        &mut self,
        name: String,
        file: PathBuf,
        location: SourceLocation,
    ) {
        self.error(
            DiagnosticId::InvalidType,
            format!(
                "Values of type '{}' can't be captured by recover expressions",
                name
            ),
            file,
            location,
        );
    }

    pub(crate) fn unsendable_field_value(
        &mut self,
        field_name: &str,
        type_name: String,
        file: PathBuf,
        location: SourceLocation,
    ) {
        self.error(
            DiagnosticId::InvalidSymbol,
            format!(
                "The field '{}' can't be assigned a value of type '{}', \
                as it's not sendable",
                field_name, type_name
            ),
            file,
            location,
        );
    }

    pub(crate) fn unsendable_return_type(
        &mut self,
        name: &str,
        type_name: String,
        file: PathBuf,
        location: SourceLocation,
    ) {
        self.error(
            DiagnosticId::InvalidCall,
            format!(
                "The method '{}' isn't available because its receiver is a \
                unique value, and the return type ('{}') isn't sendable",
                name, type_name
            ),
            file,
            location,
        );
    }

    pub(crate) fn unsendable_throw_type(
        &mut self,
        name: &str,
        type_name: String,
        file: PathBuf,
        location: SourceLocation,
    ) {
        self.error(
            DiagnosticId::InvalidCall,
            format!(
                "The method '{}' isn't available because its receiver is a \
                unique value, and the throw type ('{}') isn't sendable",
                name, type_name
            ),
            file,
            location,
        )
    }

    pub(crate) fn self_in_closure_in_recover(
        &mut self,
        file: PathBuf,
        location: SourceLocation,
    ) {
        self.error(
            DiagnosticId::InvalidType,
            "Closures inside a 'recover' can't capture or use 'self'",
            file,
            location,
        );
    }

    pub(crate) fn invalid_const_expression(
        &mut self,
        left: &str,
        operator: &str,
        right: &str,
        file: PathBuf,
        location: SourceLocation,
    ) {
        self.error(
            DiagnosticId::InvalidConstExpr,
            format!(
                "The constant expression '{} {} {}' is invalid",
                left, operator, right
            ),
            file,
            location,
        );
    }

    pub(crate) fn moved_variable(
        &mut self,
        name: &str,
        file: PathBuf,
        location: SourceLocation,
    ) {
        self.error(
            DiagnosticId::MovedVariable,
            format!("'{}' can't be used as it has been moved", name),
            file,
            location,
        );
    }

    pub(crate) fn implicit_receiver_moved(
        &mut self,
        name: &str,
        file: PathBuf,
        location: SourceLocation,
    ) {
        self.error(
            DiagnosticId::MovedVariable,
            format!("'{}' can't be used, as 'self' has been moved", name),
            file,
            location,
        );
    }

    pub(crate) fn moved_while_captured(
        &mut self,
        name: &str,
        file: PathBuf,
        location: SourceLocation,
    ) {
        self.error(
            DiagnosticId::InvalidMove,
            format!(
                "This closure can't capture '{}', as '{}' has been moved",
                name, name,
            ),
            file,
            location,
        );
    }

    pub(crate) fn moved_variable_in_loop(
        &mut self,
        name: &str,
        file: PathBuf,
        location: SourceLocation,
    ) {
        self.error(
            DiagnosticId::InvalidMove,
            format!(
                "'{}' can't be moved inside a loop, as its value \
                would be unavailable in the next iteration",
                name
            ),
            file,
            location,
        );
    }

    pub(crate) fn cant_infer_type(
        &mut self,
        name: String,
        file: PathBuf,
        location: SourceLocation,
    ) {
        self.error(
            DiagnosticId::InvalidType,
            format!(
                "The type of this expression ('{}') can't be fully inferred",
                name,
            ),
            file,
            location,
        );
    }

    pub(crate) fn cant_infer_throw_type(
        &mut self,
        file: PathBuf,
        location: SourceLocation,
    ) {
        self.error(
            DiagnosticId::InvalidType,
            "The throw type of this expression can't be inferred",
            file,
            location,
        );
    }

    pub(crate) fn cant_assign_type(
        &mut self,
        name: &str,
        file: PathBuf,
        location: SourceLocation,
    ) {
        self.error(
            DiagnosticId::InvalidType,
            format!(
                "Values of type '{}' can't be assigned to variables or fields",
                name
            ),
            file,
            location,
        )
    }

    pub(crate) fn string_literal_too_large(
        &mut self,
        limit: usize,
        file: PathBuf,
        location: SourceLocation,
    ) {
        self.error(
            DiagnosticId::LimitReached,
            format!("String literals can't be greater than {} bytes", limit),
            file,
            location,
        );
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = &Diagnostic> {
        self.values.iter()
    }
}
