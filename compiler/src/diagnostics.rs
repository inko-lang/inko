//! Types and methods for producing compiler diagnostics.
use location::Location;
use std::env;
use std::fmt;
use std::io::{stdout, IsTerminal as _, Write as _};
use std::path::PathBuf;

pub(crate) fn enable_colors() -> bool {
    env::var_os("NO_COLOR").is_none()
}

pub fn info(label: &str, message: &str) {
    // We write to STDOUT because diagnostics go to STDERR, such that one can
    // filter the output.
    let mut out = stdout().lock();
    let _ = if out.is_terminal() && enable_colors() {
        writeln!(out, "\x1b[1m{}\x1b[0m {}", label, message)
    } else {
        writeln!(out, "{} {}", label, message)
    };
}

/// The unique ID of a diagnostic.
#[derive(PartialEq, Eq, Copy, Clone)]
pub(crate) enum DiagnosticId {
    DuplicateSymbol,
    InvalidAssign,
    InvalidCall,
    InvalidCast,
    InvalidConstExpr,
    InvalidFile,
    InvalidImplementation,
    InvalidLoopKeyword,
    InvalidMatch,
    InvalidMethod,
    InvalidPattern,
    InvalidSymbol,
    InvalidSyntax,
    InvalidThrow,
    InvalidType,
    MissingField,
    MissingMain,
    MissingTrait,
    Moved,
    Unreachable,
    UnusedSymbol,
    UnusedResult,
    BorrowValueType,
    InvalidField,
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
            DiagnosticId::InvalidMethod => "invalid-method",
            DiagnosticId::InvalidImplementation => "invalid-implementation",
            DiagnosticId::InvalidAssign => "invalid-assign",
            DiagnosticId::InvalidLoopKeyword => "invalid-loop-keyword",
            DiagnosticId::InvalidThrow => "invalid-throw",
            DiagnosticId::MissingField => "missing-field",
            DiagnosticId::InvalidPattern => "invalid-pattern",
            DiagnosticId::Unreachable => "unreachable",
            DiagnosticId::Moved => "moved",
            DiagnosticId::InvalidMatch => "invalid-match",
            DiagnosticId::MissingMain => "missing-main",
            DiagnosticId::InvalidCast => "invalid-cast",
            DiagnosticId::UnusedSymbol => "unused-symbol",
            DiagnosticId::UnusedResult => "unused-result",
            DiagnosticId::BorrowValueType => "borrow-value-type",
            DiagnosticId::InvalidField => "invalid-field",
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
    location: Location,
}

impl Diagnostic {
    pub(crate) fn new(
        kind: DiagnosticType,
        id: DiagnosticId,
        message: String,
        file: PathBuf,
        location: Location,
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

    pub(crate) fn message(&self) -> &str {
        &self.message
    }

    pub(crate) fn file(&self) -> &PathBuf {
        &self.file
    }

    pub(crate) fn location(&self) -> &Location {
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
        location: Location,
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
        location: Location,
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
        location: Location,
    ) {
        self.error(
            DiagnosticId::InvalidSymbol,
            format!("the symbol '{}' is undefined", name),
            file,
            location,
        );
    }

    pub(crate) fn undefined_field(
        &mut self,
        name: &str,
        file: PathBuf,
        location: Location,
    ) {
        self.error(
            DiagnosticId::InvalidSymbol,
            format!("the field '{}' is undefined", name),
            file,
            location,
        );
    }

    pub(crate) fn unavailable_process_field(
        &mut self,
        name: &str,
        file: PathBuf,
        location: Location,
    ) {
        self.error(
            DiagnosticId::InvalidSymbol,
            format!(
                "the field '{}' can only be used by the owning process",
                name
            ),
            file,
            location,
        );
    }

    pub(crate) fn duplicate_symbol(
        &mut self,
        name: &str,
        file: PathBuf,
        location: Location,
    ) {
        self.error(
            DiagnosticId::DuplicateSymbol,
            format!("the symbol '{}' is already defined", name),
            file,
            location,
        );
    }

    pub(crate) fn duplicate_field(
        &mut self,
        name: &str,
        file: PathBuf,
        location: Location,
    ) {
        self.error(
            DiagnosticId::DuplicateSymbol,
            format!("the field '{}' is already defined", name),
            file,
            location,
        );
    }

    pub(crate) fn not_a_copy_type(
        &mut self,
        name: &str,
        file: PathBuf,
        location: Location,
    ) {
        self.error(
            DiagnosticId::InvalidType,
            format!(
                "a 'copy' or 'extern' type is expected, but '{}' is a heap \
                type",
                name
            ),
            file,
            location,
        );
    }

    pub(crate) fn not_a_value_type(
        &mut self,
        name: &str,
        file: PathBuf,
        location: Location,
    ) {
        self.error(
            DiagnosticId::InvalidType,
            format!("a value type is expected, but '{}' is a heap type", name),
            file,
            location,
        );
    }

    pub(crate) fn fields_not_allowed(
        &mut self,
        name: &str,
        file: PathBuf,
        location: Location,
    ) {
        self.error(
            DiagnosticId::InvalidField,
            format!("fields can't be defined for '{}'", name),
            file,
            location,
        );
    }

    pub(crate) fn mutable_field_not_allowed(
        &mut self,
        name: &str,
        file: PathBuf,
        location: Location,
    ) {
        self.error(
            DiagnosticId::InvalidField,
            format!("the type '{}' doesn't allow for mutable fields", name),
            file,
            location,
        );
    }

    pub(crate) fn invalid_field_assignment(
        &mut self,
        name: &str,
        file: PathBuf,
        location: Location,
    ) {
        self.error(
            DiagnosticId::InvalidAssign,
            format!(
                "values of type '{}' don't allow fields to be assigned \
                new values",
                name,
            ),
            file,
            location,
        );
    }

    pub(crate) fn immutable_field_assignment(
        &mut self,
        name: &str,
        file: PathBuf,
        location: Location,
    ) {
        self.error(
            DiagnosticId::InvalidAssign,
            format!(
                "the field '{}' is immutable and can't be assigned a new value",
                name
            ),
            file,
            location,
        );
    }

    pub(crate) fn public_field_private_type(
        &mut self,
        file: PathBuf,
        location: Location,
    ) {
        self.error(
            DiagnosticId::InvalidField,
            "public fields can't be defined for private types",
            file,
            location,
        );
    }

    pub(crate) fn duplicate_type_parameter(
        &mut self,
        name: &str,
        file: PathBuf,
        location: Location,
    ) {
        self.error(
            DiagnosticId::DuplicateSymbol,
            format!("the type parameter '{}' is already defined", name),
            file,
            location,
        );
    }

    pub(crate) fn not_a_type(
        &mut self,
        name: &str,
        file: PathBuf,
        location: Location,
    ) {
        self.error(
            DiagnosticId::InvalidType,
            format!("'{}' isn't a type", name),
            file,
            location,
        );
    }

    pub(crate) fn not_a_module(
        &mut self,
        name: &str,
        file: PathBuf,
        location: Location,
    ) {
        self.error(
            DiagnosticId::InvalidSymbol,
            format!("the symbol '{}' isn't a module", name),
            file,
            location,
        );
    }

    pub(crate) fn duplicate_method(
        &mut self,
        method_name: &str,
        type_name: String,
        file: PathBuf,
        location: Location,
    ) {
        self.error(
            DiagnosticId::DuplicateSymbol,
            format!(
                "the method '{}' is already defined for type '{}'",
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
        location: Location,
    ) {
        self.error(
            DiagnosticId::InvalidCall,
            format!("the method '{}' exists but is private", name),
            file,
            location,
        );
    }

    pub(crate) fn private_field(
        &mut self,
        name: &str,
        file: PathBuf,
        location: Location,
    ) {
        self.error(
            DiagnosticId::InvalidSymbol,
            format!("the field '{}' is private", name),
            file,
            location,
        );
    }

    pub(crate) fn type_error(
        &mut self,
        given: String,
        expected: String,
        file: PathBuf,
        location: Location,
    ) {
        self.error(
            DiagnosticId::InvalidType,
            format!(
                "expected a value of type '{}', found '{}'",
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
        location: Location,
    ) {
        self.error(
            DiagnosticId::InvalidType,
            format!(
                "this pattern expects a value of type '{}', \
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
        location: Location,
    ) {
        self.error(
            DiagnosticId::InvalidSymbol,
            format!(
                "the method '{}' isn't defined for type '{}'",
                name, receiver
            ),
            file,
            location,
        );
    }

    pub(crate) fn intrinsic_not_available(
        &mut self,
        file: PathBuf,
        location: Location,
    ) {
        self.error(
            DiagnosticId::InvalidCall,
            "intrinsics can only be used in the standard library",
            file,
            location,
        );
    }

    pub(crate) fn tuple_size_error(
        &mut self,
        file: PathBuf,
        location: Location,
    ) {
        self.error(
            DiagnosticId::InvalidType,
            "tuples are limited to up to 8 members",
            file,
            location,
        );
    }

    pub(crate) fn incorrect_call_arguments(
        &mut self,
        given: usize,
        expected: usize,
        file: PathBuf,
        location: Location,
    ) {
        self.error(
            DiagnosticId::InvalidCall,
            format!(
                "incorrect number of arguments: expected {}, found {}",
                expected, given
            ),
            file,
            location,
        );
    }

    pub(crate) fn closure_with_named_argument(
        &mut self,
        file: PathBuf,
        location: Location,
    ) {
        self.error(
            DiagnosticId::InvalidCall,
            "closures don't support named arguments",
            file,
            location,
        );
    }

    pub(crate) fn incorrect_pattern_arguments(
        &mut self,
        given: usize,
        expected: usize,
        file: PathBuf,
        location: Location,
    ) {
        self.error(
            DiagnosticId::InvalidPattern,
            format!(
                "incorrect number of pattern arguments: expected {}, found {}",
                expected, given
            ),
            file,
            location,
        );
    }

    pub(crate) fn undefined_constructor(
        &mut self,
        name: &str,
        type_name: String,
        file: PathBuf,
        location: Location,
    ) {
        self.error(
            DiagnosticId::InvalidSymbol,
            format!(
                "the constructor '{}' doesn't exist for type '{}'",
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
        location: Location,
    ) {
        self.error(
            DiagnosticId::InvalidSymbol,
            format!("the symbol '{}' isn't a module", name),
            file,
            location,
        );
    }

    pub(crate) fn symbol_not_a_value(
        &mut self,
        name: &str,
        file: PathBuf,
        location: Location,
    ) {
        self.error(
            DiagnosticId::InvalidSymbol,
            format!("the symbol '{}' is defined but isn't a value", name),
            file,
            location,
        )
    }

    pub(crate) fn invalid_instance_call(
        &mut self,
        name: &str,
        receiver: String,
        file: PathBuf,
        location: Location,
    ) {
        self.error(
            DiagnosticId::InvalidCall,
            format!(
                "the method '{}' exists for type '{}', \
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
        location: Location,
    ) {
        self.error(
            DiagnosticId::InvalidCall,
            format!(
                "the method '{}' exists for type '{}', \
                but is a static method",
                name, receiver,
            ),
            file,
            location,
        );
    }

    pub(crate) fn unreachable(&mut self, file: PathBuf, location: Location) {
        self.warn(
            DiagnosticId::Unreachable,
            "this code is unreachable",
            file,
            location,
        );
    }

    pub(crate) fn unreachable_pattern(
        &mut self,
        file: PathBuf,
        location: Location,
    ) {
        self.warn(
            DiagnosticId::Unreachable,
            "this pattern is unreachable",
            file,
            location,
        );
    }

    pub(crate) fn unsendable_argument(
        &mut self,
        argument: String,
        file: PathBuf,
        location: Location,
    ) {
        self.error(
            DiagnosticId::InvalidType,
            format!(
                "the receiver of this call requires sendable arguments, \
                but '{}' isn't sendable",
                argument,
            ),
            file,
            location,
        );
    }

    pub(crate) fn unsendable_return_type(
        &mut self,
        name: String,
        file: PathBuf,
        location: Location,
    ) {
        self.error(
            DiagnosticId::InvalidCall,
            format!(
                "the receiver of this call requires a sendable return type, \
                but '{}' isn't sendable",
                name
            ),
            file,
            location,
        );
    }

    pub(crate) fn unsendable_old_value(
        &mut self,
        name: &str,
        file: PathBuf,
        location: Location,
    ) {
        self.error(
            DiagnosticId::InvalidAssign,
            format!(
                "the value of '{}' can't be replaced inside a 'recover', \
                as the old value isn't sendable",
                name
            ),
            file,
            location,
        );
    }

    pub(crate) fn unsendable_async_type(
        &mut self,
        name: String,
        file: PathBuf,
        location: Location,
    ) {
        self.error(
            DiagnosticId::InvalidType,
            format!(
                "values of type '{}' can't be sent between processes",
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
        location: Location,
    ) {
        self.error(
            DiagnosticId::InvalidAssign,
            format!(
                "the field '{}' can't be assigned a value of type '{}', \
                as it's not sendable",
                field_name, type_name
            ),
            file,
            location,
        );
    }

    pub(crate) fn self_in_closure_in_recover(
        &mut self,
        file: PathBuf,
        location: Location,
    ) {
        self.error(
            DiagnosticId::InvalidType,
            "closures inside a 'recover' can't capture or use 'self'",
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
        location: Location,
    ) {
        self.error(
            DiagnosticId::InvalidConstExpr,
            format!(
                "the constant expression '{} {} {}' is invalid",
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
        location: Location,
    ) {
        self.error(
            DiagnosticId::Moved,
            format!("'{}' can't be used as it has been moved", name),
            file,
            location,
        );
    }

    pub(crate) fn call_moves_receiver_as_argument(
        &mut self,
        name: &str,
        file: PathBuf,
        location: Location,
    ) {
        self.error(
            DiagnosticId::InvalidCall,
            format!(
                "the method '{}' can't be called because it borrows its \
                receiver while also moving it as part of an argument",
                name
            ),
            file,
            location,
        );
    }

    pub(crate) fn implicit_receiver_moved(
        &mut self,
        name: &str,
        file: PathBuf,
        location: Location,
    ) {
        self.error(
            DiagnosticId::Moved,
            format!("'{}' can't be used, as 'self' has been moved", name),
            file,
            location,
        );
    }

    pub(crate) fn moved_while_captured(
        &mut self,
        name: &str,
        file: PathBuf,
        location: Location,
    ) {
        self.error(
            DiagnosticId::Moved,
            format!(
                "this closure can't capture '{}', as '{}' has been moved",
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
        location: Location,
    ) {
        self.error(
            DiagnosticId::Moved,
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
        location: Location,
    ) {
        self.error(
            DiagnosticId::InvalidType,
            format!(
                "the type of this expression ('{}') can't be fully inferred",
                name,
            ),
            file,
            location,
        );
    }

    pub(crate) fn type_depth_exceeded(
        &mut self,
        name: String,
        file: PathBuf,
        location: Location,
    ) {
        self.error(
            DiagnosticId::InvalidType,
            format!(
                "the type of this expression ('{}') can't be fully inferred \
                as it contains too many nested types",
                name,
            ),
            file,
            location,
        );
    }

    pub(crate) fn type_containing_uni_value_borrow(
        &mut self,
        name: String,
        file: PathBuf,
        location: Location,
    ) {
        self.error(
            DiagnosticId::InvalidType,
            format!(
                "the type of this expression ('{}') is invalid because it is \
                or contains a borrow of a 'uni T' value",
                name,
            ),
            file,
            location,
        );
    }

    pub(crate) fn invalid_borrow(
        &mut self,
        name: String,
        file: PathBuf,
        location: Location,
    ) {
        self.error(
            DiagnosticId::InvalidType,
            format!("values of type '{}' can't be borrowed", name,),
            file,
            location,
        );
    }

    pub(crate) fn default_method_capturing_self(
        &mut self,
        file: PathBuf,
        location: Location,
    ) {
        self.error(
            DiagnosticId::InvalidType,
            "closures defined in default methods can't capture borrows of \
            'self'",
            file,
            location,
        );
    }

    pub(crate) fn duplicate_type_parameter_requirement(
        &mut self,
        param: &str,
        req: &str,
        file: PathBuf,
        location: Location,
    ) {
        self.error(
            DiagnosticId::InvalidType,
            format!(
                "type parameter '{}' already defines the '{}' requirement",
                param, req
            ),
            file,
            location,
        );
    }

    pub(crate) fn mutable_copy_type_parameter(
        &mut self,
        file: PathBuf,
        location: Location,
    ) {
        self.error(
            DiagnosticId::InvalidType,
            "type parameters can't be both 'mut' and 'copy', as 'copy' types \
            are immutable"
                .to_string(),
            file,
            location,
        );
    }

    pub(crate) fn invalid_try(
        &mut self,
        name: String,
        file: PathBuf,
        location: Location,
    ) {
        self.error(
            DiagnosticId::InvalidThrow,
            format!(
                "this expression must return an 'Option' or 'Result', \
                found '{}'",
                name
            ),
            file,
            location,
        );
    }

    pub(crate) fn try_not_available(
        &mut self,
        file: PathBuf,
        location: Location,
    ) {
        self.error(
            DiagnosticId::InvalidThrow,
            "'try' can only be used in methods that return an \
            'Option' or 'Result'",
            file,
            location,
        );
    }

    pub(crate) fn throw_not_available(
        &mut self,
        file: PathBuf,
        location: Location,
    ) {
        self.error(
            DiagnosticId::InvalidThrow,
            "'throw' can only be used in methods that return a 'Result'",
            file,
            location,
        );
    }

    pub(crate) fn invalid_throw(
        &mut self,
        error: String,
        returns: String,
        file: PathBuf,
        location: Location,
    ) {
        self.error(
            DiagnosticId::InvalidThrow,
            format!(
                "can't throw '{}' as the surrounding method \
                returns a '{}'",
                error, returns,
            ),
            file,
            location,
        );
    }

    pub(crate) fn incorrect_number_of_type_arguments(
        &mut self,
        required: usize,
        given: usize,
        file: PathBuf,
        location: Location,
    ) {
        self.error(
            DiagnosticId::InvalidType,
            format!(
                "incorrect number of type arguments: expected {}, found {}",
                required, given
            ),
            file,
            location,
        );
    }

    pub(crate) fn invalid_c_type(
        &mut self,
        name: &str,
        file: PathBuf,
        location: Location,
    ) {
        self.error(
            DiagnosticId::InvalidType,
            format!("'{}' isn't a valid C type", name),
            file,
            location,
        );
    }

    pub(crate) fn unused_symbol(
        &mut self,
        name: &str,
        file: PathBuf,
        location: Location,
    ) {
        self.warn(
            DiagnosticId::UnusedSymbol,
            format!("the symbol '{}' is unused", name),
            file,
            location,
        );
    }

    pub(crate) fn invalid_inline_method(
        &mut self,
        file: PathBuf,
        location: Location,
    ) {
        self.error(
            DiagnosticId::InvalidMethod,
            "the 'inline' keyword can't be used for this type of method",
            file,
            location,
        );
    }

    pub(crate) fn invalid_mut_type(
        &mut self,
        name: &str,
        file: PathBuf,
        location: Location,
    ) {
        self.error(
            DiagnosticId::InvalidType,
            format!(
                "mutable borrows of type '{}' are invalid as '{}' is immutable",
                name, name
            ),
            file,
            location,
        );
    }

    pub(crate) fn unused_result(&mut self, file: PathBuf, location: Location) {
        self.warn(
            DiagnosticId::UnusedResult,
            "the result of this expression is unused",
            file,
            location,
        );
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = &Diagnostic> {
        self.values.iter()
    }
}
