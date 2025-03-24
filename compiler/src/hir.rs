//! Inko's high-level typed intermediate representation, or HIR for short.
//!
//! HIR is generated from the AST, and share many similarities with it. Unlike
//! the AST it stores type information, and some AST nodes are desugared into
//! different HIR nodes.
use crate::diagnostics::DiagnosticId;
use crate::modules_parser::ParsedModule;
use crate::state::State;
use ::ast::nodes::{self as ast, Node as _};
use location::Location;
use std::path::PathBuf;
use std::str::FromStr;
use types::{
    ARRAY_INTERNAL_NAME, ARRAY_PUSH, ARRAY_WITH_CAPACITY,
    STRING_BUFFER_INTERNAL_NAME, STRING_BUFFER_INTO_STRING, STRING_BUFFER_NEW,
    STRING_BUFFER_PUSH, TO_STRING_METHOD,
};

const BUILTIN_RECEIVER: &str = "_INKO";
const ARRAY_LIT_VAR: &str = "$array";
const ITER_VAR: &str = "$iter";
const NEXT_CALL: &str = "next";
const SOME_CONS: &str = "Some";
const INTO_ITER_CALL: &str = "into_iter";
const STR_BUF_VAR: &str = "$buf";

struct Comments {
    nodes: Vec<ast::Comment>,
}

impl Comments {
    fn new() -> Comments {
        Comments { nodes: Vec::new() }
    }

    fn push(&mut self, comment: ast::Comment) {
        let end = comment.location.line_end;
        let last = self.nodes.last().map_or(0, |c| c.location.line_end);

        if end - last > 1 && !self.nodes.is_empty() {
            self.nodes.clear();
        }

        self.nodes.push(comment);
    }

    fn documentation_for(&mut self, location: &Location) -> String {
        let should_take = self
            .nodes
            .last()
            .map_or(false, |c| location.line_start - c.location.line_end == 1);

        if should_take {
            self.generate()
        } else {
            if !self.nodes.is_empty() {
                self.nodes.clear();
            }

            String::new()
        }
    }

    fn generate(&mut self) -> String {
        let mut docs = String::new();

        for node in &self.nodes {
            if !docs.is_empty() {
                docs.push('\n');
            }

            docs.push_str(&node.value);
        }

        self.nodes.clear();
        docs
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum Usage {
    Unused,
    Discarded,
    Used,
}

impl Usage {
    pub(crate) fn is_used(self) -> bool {
        matches!(self, Usage::Used)
    }

    pub(crate) fn is_unused(self) -> bool {
        matches!(self, Usage::Unused)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct IntLiteral {
    pub(crate) value: i64,
    pub(crate) resolved_type: types::TypeRef,
    pub(crate) location: Location,
}

#[derive(Clone, Debug)]
pub(crate) struct FloatLiteral {
    pub(crate) value: f64,
    pub(crate) resolved_type: types::TypeRef,
    pub(crate) location: Location,
}

impl PartialEq for FloatLiteral {
    fn eq(&self, other: &Self) -> bool {
        // This is just to make unit testing easier.
        self.value == other.value && self.location == other.location
    }
}

impl Eq for FloatLiteral {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct StringLiteral {
    pub(crate) value: String,
    pub(crate) resolved_type: types::TypeRef,
    pub(crate) location: Location,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ConstStringLiteral {
    pub(crate) value: String,
    pub(crate) resolved_type: types::TypeRef,
    pub(crate) location: Location,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TupleLiteral {
    pub(crate) type_id: Option<types::TypeId>,
    pub(crate) value_types: Vec<types::TypeRef>,
    pub(crate) resolved_type: types::TypeRef,
    pub(crate) values: Vec<Expression>,
    pub(crate) location: Location,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Identifier {
    pub(crate) name: String,
    pub(crate) location: Location,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Constant {
    pub(crate) name: String,
    pub(crate) location: Location,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ConstantRef {
    pub(crate) kind: types::ConstantKind,
    pub(crate) source: Option<Identifier>,
    pub(crate) name: String,
    pub(crate) resolved_type: types::TypeRef,
    pub(crate) usage: Usage,
    pub(crate) location: Location,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct IdentifierRef {
    pub(crate) name: String,
    pub(crate) kind: types::IdentifierKind,
    pub(crate) usage: Usage,
    pub(crate) location: Location,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Call {
    pub(crate) kind: types::CallKind,
    pub(crate) receiver: Option<Expression>,
    pub(crate) name: Identifier,
    pub(crate) arguments: Vec<Argument>,
    /// A flag that signals parentheses are used, even if no arguments are
    /// specified. This is used to disambiguate between `Foo` referring to a
    /// type, and `Foo()` that creates an instance of a type.
    ///
    /// We use this flag instead of turning `arguments` into
    /// `Option<Vec<Argument>>` since we don't care about the presence (or lack)
    /// of parentheses 99% of the time.
    pub(crate) parens: bool,
    /// A flag indicating if the call resides directly in a `mut` expression.
    pub(crate) in_mut: bool,
    pub(crate) usage: Usage,
    pub(crate) location: Location,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BuiltinCall {
    pub(crate) info: Option<types::IntrinsicCall>,
    pub(crate) name: Identifier,
    pub(crate) arguments: Vec<Expression>,
    pub(crate) location: Location,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AssignField {
    pub(crate) field_id: Option<types::FieldId>,
    pub(crate) resolved_type: types::TypeRef,
    pub(crate) field: Field,
    pub(crate) value: Expression,
    pub(crate) location: Location,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ReplaceField {
    pub(crate) field_id: Option<types::FieldId>,
    pub(crate) resolved_type: types::TypeRef,
    pub(crate) field: Field,
    pub(crate) value: Expression,
    pub(crate) location: Location,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AssignVariable {
    pub(crate) variable_id: Option<types::VariableId>,
    pub(crate) resolved_type: types::TypeRef,
    pub(crate) variable: Identifier,
    pub(crate) value: Expression,
    pub(crate) location: Location,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ReplaceVariable {
    pub(crate) variable_id: Option<types::VariableId>,
    pub(crate) resolved_type: types::TypeRef,
    pub(crate) variable: Identifier,
    pub(crate) value: Expression,
    pub(crate) location: Location,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AssignSetter {
    pub(crate) kind: types::CallKind,
    pub(crate) receiver: Expression,
    pub(crate) name: Identifier,
    pub(crate) value: Expression,
    pub(crate) location: Location,
    pub(crate) usage: Usage,
    pub(crate) expected_type: types::TypeRef,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ReplaceSetter {
    pub(crate) field_id: Option<types::FieldId>,
    pub(crate) receiver: Expression,
    pub(crate) name: Identifier,
    pub(crate) value: Expression,
    pub(crate) location: Location,
    pub(crate) resolved_type: types::TypeRef,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ImportSymbol {
    pub(crate) name: Identifier,
    pub(crate) import_as: Identifier,
    pub(crate) location: Location,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Import {
    pub(crate) source: Vec<Identifier>,
    pub(crate) symbols: Vec<ImportSymbol>,
    pub(crate) location: Location,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ExternImport {
    pub(crate) source: String,
    pub(crate) location: Location,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DefineConstant {
    pub(crate) public: bool,
    pub(crate) documentation: String,
    pub(crate) constant_id: Option<types::ConstantId>,
    pub(crate) name: Constant,
    pub(crate) value: ConstExpression,
    pub(crate) location: Location,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum MethodKind {
    Regular,
    Moving,
    Mutable,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DefineInstanceMethod {
    pub(crate) documentation: String,
    pub(crate) public: bool,
    pub(crate) inline: bool,
    pub(crate) kind: MethodKind,
    pub(crate) name: Identifier,
    pub(crate) type_parameters: Vec<TypeParameter>,
    pub(crate) arguments: Vec<MethodArgument>,
    pub(crate) return_type: Option<Type>,
    pub(crate) body: Vec<Expression>,
    pub(crate) location: Location,
    pub(crate) method_id: Option<types::MethodId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DefineModuleMethod {
    pub(crate) documentation: String,
    pub(crate) public: bool,
    pub(crate) inline: bool,
    pub(crate) c_calling_convention: bool,
    pub(crate) name: Identifier,
    pub(crate) type_parameters: Vec<TypeParameter>,
    pub(crate) arguments: Vec<MethodArgument>,
    pub(crate) return_type: Option<Type>,
    pub(crate) body: Vec<Expression>,
    pub(crate) location: Location,
    pub(crate) method_id: Option<types::MethodId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DefineExternFunction {
    pub(crate) documentation: String,
    pub(crate) public: bool,
    pub(crate) name: Identifier,
    pub(crate) arguments: Vec<MethodArgument>,
    pub(crate) variadic: bool,
    pub(crate) return_type: Option<Type>,
    pub(crate) location: Location,
    pub(crate) method_id: Option<types::MethodId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DefineRequiredMethod {
    pub(crate) documentation: String,
    pub(crate) public: bool,
    pub(crate) kind: MethodKind,
    pub(crate) name: Identifier,
    pub(crate) type_parameters: Vec<TypeParameter>,
    pub(crate) arguments: Vec<MethodArgument>,
    pub(crate) return_type: Option<Type>,
    pub(crate) method_id: Option<types::MethodId>,
    pub(crate) location: Location,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DefineStaticMethod {
    pub(crate) documentation: String,
    pub(crate) public: bool,
    pub(crate) inline: bool,
    pub(crate) name: Identifier,
    pub(crate) type_parameters: Vec<TypeParameter>,
    pub(crate) arguments: Vec<MethodArgument>,
    pub(crate) return_type: Option<Type>,
    pub(crate) body: Vec<Expression>,
    pub(crate) location: Location,
    pub(crate) method_id: Option<types::MethodId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DefineAsyncMethod {
    pub(crate) documentation: String,
    pub(crate) mutable: bool,
    pub(crate) public: bool,
    pub(crate) name: Identifier,
    pub(crate) type_parameters: Vec<TypeParameter>,
    pub(crate) arguments: Vec<MethodArgument>,
    pub(crate) return_type: Option<Type>,
    pub(crate) body: Vec<Expression>,
    pub(crate) location: Location,
    pub(crate) method_id: Option<types::MethodId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DefineField {
    pub(crate) documentation: String,
    pub(crate) public: bool,
    pub(crate) mutable: bool,
    pub(crate) field_id: Option<types::FieldId>,
    pub(crate) name: Identifier,
    pub(crate) value_type: Type,
    pub(crate) location: Location,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum TypeExpression {
    InstanceMethod(Box<DefineInstanceMethod>),
    StaticMethod(Box<DefineStaticMethod>),
    AsyncMethod(Box<DefineAsyncMethod>),
    Field(Box<DefineField>),
    Constructor(Box<DefineConstructor>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum TypeKind {
    Async,
    Builtin,
    Enum,
    Regular,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum TypeSemantics {
    Default,
    Inline,
    Copy,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DefineType {
    pub(crate) documentation: String,
    pub(crate) public: bool,
    pub(crate) semantics: TypeSemantics,
    pub(crate) type_id: Option<types::TypeId>,
    pub(crate) kind: TypeKind,
    pub(crate) name: Constant,
    pub(crate) type_parameters: Vec<TypeParameter>,
    pub(crate) body: Vec<TypeExpression>,
    pub(crate) location: Location,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DefineExternType {
    pub(crate) documentation: String,
    pub(crate) public: bool,
    pub(crate) type_id: Option<types::TypeId>,
    pub(crate) name: Constant,
    pub(crate) fields: Vec<DefineField>,
    pub(crate) location: Location,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DefineConstructor {
    pub(crate) documentation: String,
    pub(crate) method_id: Option<types::MethodId>,
    pub(crate) constructor_id: Option<types::ConstructorId>,
    pub(crate) name: Constant,
    pub(crate) members: Vec<Type>,
    pub(crate) location: Location,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum TraitExpression {
    InstanceMethod(Box<DefineInstanceMethod>),
    RequiredMethod(Box<DefineRequiredMethod>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DefineTrait {
    pub(crate) documentation: String,
    pub(crate) public: bool,
    pub(crate) trait_id: Option<types::TraitId>,
    pub(crate) name: Constant,
    pub(crate) type_parameters: Vec<TypeParameter>,
    pub(crate) requirements: Vec<TypeName>,
    pub(crate) body: Vec<TraitExpression>,
    pub(crate) location: Location,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum TopLevelExpression {
    Type(Box<DefineType>),
    ExternType(Box<DefineExternType>),
    Constant(Box<DefineConstant>),
    ModuleMethod(Box<DefineModuleMethod>),
    ExternFunction(Box<DefineExternFunction>),
    Trait(Box<DefineTrait>),
    Implement(Box<ImplementTrait>),
    Import(Box<Import>),
    Reopen(Box<ReopenType>),
    ExternImport(Box<ExternImport>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ReopenType {
    pub(crate) type_id: Option<types::TypeId>,
    pub(crate) type_name: Constant,
    pub(crate) body: Vec<ReopenTypeExpression>,
    pub(crate) bounds: Vec<TypeBound>,
    pub(crate) location: Location,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ReopenTypeExpression {
    InstanceMethod(Box<DefineInstanceMethod>),
    StaticMethod(Box<DefineStaticMethod>),
    AsyncMethod(Box<DefineAsyncMethod>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TypeBound {
    pub(crate) name: Constant,
    pub(crate) requirements: Vec<TypeName>,
    pub(crate) mutable: bool,
    pub(crate) copy: bool,
    pub(crate) location: Location,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ImplementTrait {
    pub(crate) trait_name: TypeName,
    pub(crate) type_name: Constant,
    pub(crate) body: Vec<DefineInstanceMethod>,
    pub(crate) location: Location,
    pub(crate) bounds: Vec<TypeBound>,
    pub(crate) trait_instance: Option<types::TraitInstance>,
    pub(crate) type_instance: Option<types::TypeInstance>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Scope {
    pub(crate) resolved_type: types::TypeRef,
    pub(crate) body: Vec<Expression>,
    pub(crate) location: Location,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Try {
    pub(crate) expression: Expression,
    pub(crate) location: Location,
    pub(crate) kind: types::ThrowKind,
    pub(crate) return_type: types::TypeRef,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SizeOf {
    pub(crate) argument: Type,
    pub(crate) resolved_type: types::TypeRef,
    pub(crate) location: Location,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Expression {
    And(Box<And>),
    AssignField(Box<AssignField>),
    ReplaceField(Box<ReplaceField>),
    AssignSetter(Box<AssignSetter>),
    ReplaceSetter(Box<ReplaceSetter>),
    AssignVariable(Box<AssignVariable>),
    ReplaceVariable(Box<ReplaceVariable>),
    Break(Box<Break>),
    BuiltinCall(Box<BuiltinCall>),
    Call(Box<Call>),
    Closure(Box<Closure>),
    ConstantRef(Box<ConstantRef>),
    DefineVariable(Box<DefineVariable>),
    False(Box<False>),
    FieldRef(Box<FieldRef>),
    Float(Box<FloatLiteral>),
    IdentifierRef(Box<IdentifierRef>),
    Int(Box<IntLiteral>),
    Loop(Box<Loop>),
    Match(Box<Match>),
    Mut(Box<Mut>),
    Next(Box<Next>),
    Or(Box<Or>),
    Ref(Box<Ref>),
    Return(Box<Return>),
    Scope(Box<Scope>),
    SelfObject(Box<SelfObject>),
    String(Box<StringLiteral>),
    Throw(Box<Throw>),
    True(Box<True>),
    Nil(Box<Nil>),
    Tuple(Box<TupleLiteral>),
    TypeCast(Box<TypeCast>),
    Recover(Box<Recover>),
    Try(Box<Try>),
    SizeOf(Box<SizeOf>),
}

impl Expression {
    fn call_static(
        type_name: &str,
        method_name: &str,
        arguments: Vec<Expression>,
        location: Location,
    ) -> Expression {
        Self::call(
            Expression::ConstantRef(Box::new(ConstantRef {
                kind: types::ConstantKind::Unknown,
                source: None,
                name: type_name.to_string(),
                resolved_type: types::TypeRef::Unknown,
                usage: Usage::Used,
                location,
            })),
            method_name,
            arguments,
            location,
        )
    }

    fn call(
        receiver: Expression,
        method_name: &str,
        arguments: Vec<Expression>,
        location: Location,
    ) -> Expression {
        Expression::Call(Box::new(Call {
            kind: types::CallKind::Unknown,
            receiver: Some(receiver),
            name: Identifier { name: method_name.to_string(), location },
            parens: true,
            in_mut: false,
            usage: Usage::Used,
            arguments: arguments
                .into_iter()
                .map(|e| {
                    Argument::Positional(Box::new(PositionalArgument {
                        value: e,
                        expected_type: types::TypeRef::Unknown,
                    }))
                })
                .collect(),
            location,
        }))
    }

    fn define_variable(
        name: &str,
        value: Expression,
        location: Location,
    ) -> Expression {
        Expression::DefineVariable(Box::new(DefineVariable {
            resolved_type: types::TypeRef::Unknown,
            variable_id: None,
            mutable: false,
            name: Identifier { name: name.to_string(), location },
            value_type: None,
            value,
            location,
        }))
    }

    fn identifier_ref(name: &str, location: Location) -> Expression {
        Expression::IdentifierRef(Box::new(IdentifierRef {
            name: name.to_string(),
            kind: types::IdentifierKind::Unknown,
            usage: Usage::Used,
            location,
        }))
    }

    fn string(value: String, location: Location) -> Expression {
        Expression::String(Box::new(StringLiteral {
            value,
            resolved_type: types::TypeRef::Unknown,
            location,
        }))
    }

    pub(crate) fn location(&self) -> Location {
        match self {
            Expression::And(ref n) => n.location,
            Expression::AssignField(ref n) => n.location,
            Expression::ReplaceField(ref n) => n.location,
            Expression::AssignSetter(ref n) => n.location,
            Expression::ReplaceSetter(ref n) => n.location,
            Expression::AssignVariable(ref n) => n.location,
            Expression::ReplaceVariable(ref n) => n.location,
            Expression::Break(ref n) => n.location,
            Expression::BuiltinCall(ref n) => n.location,
            Expression::Call(ref n) => n.location,
            Expression::Closure(ref n) => n.location,
            Expression::ConstantRef(ref n) => n.location,
            Expression::DefineVariable(ref n) => n.location,
            Expression::False(ref n) => n.location,
            Expression::FieldRef(ref n) => n.location,
            Expression::Float(ref n) => n.location,
            Expression::IdentifierRef(ref n) => n.location,
            Expression::Int(ref n) => n.location,
            Expression::Loop(ref n) => n.location,
            Expression::Match(ref n) => n.location,
            Expression::Mut(ref n) => n.location,
            Expression::Next(ref n) => n.location,
            Expression::Or(ref n) => n.location,
            Expression::Ref(ref n) => n.location,
            Expression::Return(ref n) => n.location,
            Expression::Scope(ref n) => n.location,
            Expression::SelfObject(ref n) => n.location,
            Expression::String(ref n) => n.location,
            Expression::Throw(ref n) => n.location,
            Expression::True(ref n) => n.location,
            Expression::Nil(ref n) => n.location,
            Expression::Tuple(ref n) => n.location,
            Expression::TypeCast(ref n) => n.location,
            Expression::Recover(ref n) => n.location,
            Expression::Try(ref n) => n.location,
            Expression::SizeOf(ref n) => n.location,
        }
    }

    pub(crate) fn returns_value(&self) -> bool {
        !matches!(
            self,
            Expression::Return(_)
                | Expression::Throw(_)
                | Expression::DefineVariable(_)
                | Expression::Loop(_)
                | Expression::Break(_)
                | Expression::Next(_)
        )
    }

    pub(crate) fn is_self(&self) -> bool {
        matches!(self, Expression::SelfObject(_))
    }

    pub(crate) fn is_recover(&self) -> bool {
        matches!(self, Expression::Recover(_))
    }

    pub(crate) fn set_usage(&mut self, usage: Usage) {
        match self {
            Expression::Call(c) => c.usage = usage,
            Expression::IdentifierRef(c) => c.usage = usage,
            Expression::ConstantRef(c) => c.usage = usage,
            Expression::AssignSetter(c) => c.usage = usage,
            _ => {}
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ConstExpression {
    Int(Box<IntLiteral>),
    String(Box<ConstStringLiteral>),
    Float(Box<FloatLiteral>),
    Binary(Box<ConstBinary>),
    ConstantRef(Box<ConstantRef>),
    Array(Box<ConstArray>),
    True(Box<True>),
    False(Box<False>),
}

impl ConstExpression {
    pub(crate) fn location(&self) -> Location {
        match self {
            Self::Int(ref n) => n.location,
            Self::String(ref n) => n.location,
            Self::Float(ref n) => n.location,
            Self::Binary(ref n) => n.location,
            Self::ConstantRef(ref n) => n.location,
            Self::Array(ref n) => n.location,
            Self::True(ref n) => n.location,
            Self::False(ref n) => n.location,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TypeParameter {
    pub(crate) type_parameter_id: Option<types::TypeParameterId>,
    pub(crate) name: Constant,
    pub(crate) requirements: Vec<TypeName>,
    pub(crate) mutable: bool,
    pub(crate) copy: bool,
    pub(crate) location: Location,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MethodArgument {
    pub(crate) name: Identifier,
    pub(crate) value_type: Type,
    pub(crate) location: Location,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PositionalArgument {
    pub(crate) value: Expression,
    pub(crate) expected_type: types::TypeRef,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct NamedArgument {
    pub(crate) index: usize,
    pub(crate) name: Identifier,
    pub(crate) value: Expression,
    pub(crate) expected_type: types::TypeRef,
    pub(crate) location: Location,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Argument {
    Positional(Box<PositionalArgument>),
    Named(Box<NamedArgument>),
}

impl Argument {
    pub fn location(&self) -> Location {
        match self {
            Argument::Positional(n) => n.value.location(),
            Argument::Named(n) => n.location,
        }
    }

    pub fn into_value(self) -> Expression {
        match self {
            Argument::Positional(n) => n.value,
            Argument::Named(n) => n.value,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TypeName {
    pub(crate) source: Option<Identifier>,
    pub(crate) resolved_type: types::TypeRef,
    pub(crate) name: Constant,
    pub(crate) arguments: Vec<Type>,
    pub(crate) location: Location,
    pub(crate) self_type: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ReferenceType {
    pub(crate) type_reference: ReferrableType,
    pub(crate) location: Location,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ReferrableType {
    Named(Box<TypeName>),
    Closure(Box<ClosureType>),
    Tuple(Box<TupleType>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ClosureType {
    pub(crate) arguments: Vec<Type>,
    pub(crate) return_type: Option<Type>,
    pub(crate) location: Location,
    pub(crate) resolved_type: types::TypeRef,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TupleType {
    pub(crate) resolved_type: types::TypeRef,
    pub(crate) values: Vec<Type>,
    pub(crate) location: Location,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Type {
    Named(Box<TypeName>),
    Ref(Box<ReferenceType>),
    Mut(Box<ReferenceType>),
    Uni(Box<ReferenceType>),
    Owned(Box<ReferenceType>),
    Closure(Box<ClosureType>),
    Tuple(Box<TupleType>),
}

impl Type {
    pub(crate) fn location(&self) -> Location {
        match self {
            Type::Named(ref node) => node.location,
            Type::Ref(ref node) => node.location,
            Type::Mut(ref node) => node.location,
            Type::Uni(ref node) => node.location,
            Type::Owned(ref node) => node.location,
            Type::Closure(ref node) => node.location,
            Type::Tuple(ref node) => node.location,
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum Operator {
    Add,
    BitAnd,
    BitOr,
    BitXor,
    Div,
    Eq,
    Ge,
    Gt,
    Le,
    Lt,
    Mod,
    Mul,
    Ne,
    Pow,
    Shl,
    Shr,
    Sub,
    UnsignedShr,
}

impl Operator {
    pub(crate) fn from_ast(kind: ast::OperatorKind) -> Operator {
        // This isn't ideal, but I also don't want to introduce a standalone
        // Operator enum in its own module _just_ so we don't need this match.
        match kind {
            ast::OperatorKind::Add => Operator::Add,
            ast::OperatorKind::BitAnd => Operator::BitAnd,
            ast::OperatorKind::BitOr => Operator::BitOr,
            ast::OperatorKind::BitXor => Operator::BitXor,
            ast::OperatorKind::Div => Operator::Div,
            ast::OperatorKind::Eq => Operator::Eq,
            ast::OperatorKind::Gt => Operator::Gt,
            ast::OperatorKind::Ge => Operator::Ge,
            ast::OperatorKind::Lt => Operator::Lt,
            ast::OperatorKind::Le => Operator::Le,
            ast::OperatorKind::Mod => Operator::Mod,
            ast::OperatorKind::Mul => Operator::Mul,
            ast::OperatorKind::Ne => Operator::Ne,
            ast::OperatorKind::Pow => Operator::Pow,
            ast::OperatorKind::Shl => Operator::Shl,
            ast::OperatorKind::Shr => Operator::Shr,
            ast::OperatorKind::Sub => Operator::Sub,
            ast::OperatorKind::UnsignedShr => Operator::UnsignedShr,
        }
    }

    pub(crate) fn method_name(self) -> &'static str {
        match self {
            Operator::Add => "+",
            Operator::BitAnd => "&",
            Operator::BitOr => "|",
            Operator::BitXor => "^",
            Operator::Div => "/",
            Operator::Mod => "%",
            Operator::Mul => "*",
            Operator::Pow => "**",
            Operator::Shl => "<<",
            Operator::Shr => ">>",
            Operator::Sub => "-",
            Operator::Eq => "==",
            Operator::Ne => "!=",
            Operator::Gt => ">",
            Operator::Ge => ">=",
            Operator::Lt => "<",
            Operator::Le => "<=",
            Operator::UnsignedShr => ">>>",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ConstBinary {
    pub(crate) left: ConstExpression,
    pub(crate) right: ConstExpression,
    pub(crate) operator: Operator,
    pub(crate) resolved_type: types::TypeRef,
    pub(crate) location: Location,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ConstArray {
    pub(crate) resolved_type: types::TypeRef,
    pub(crate) values: Vec<ConstExpression>,
    pub(crate) location: Location,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Field {
    pub(crate) name: String,
    pub(crate) location: Location,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FieldRef {
    pub(crate) info: Option<types::FieldInfo>,
    pub(crate) name: String,
    pub(crate) location: Location,
    pub(crate) in_mut: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BlockArgument {
    pub(crate) variable_id: Option<types::VariableId>,
    pub(crate) name: Identifier,
    pub(crate) value_type: Option<Type>,
    pub(crate) location: Location,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Closure {
    pub(crate) closure_id: Option<types::ClosureId>,
    pub(crate) resolved_type: types::TypeRef,
    pub(crate) moving: bool,
    pub(crate) arguments: Vec<BlockArgument>,
    pub(crate) return_type: Option<Type>,
    pub(crate) body: Vec<Expression>,
    pub(crate) location: Location,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DefineVariable {
    pub(crate) resolved_type: types::TypeRef,
    pub(crate) variable_id: Option<types::VariableId>,
    pub(crate) mutable: bool,
    pub(crate) name: Identifier,
    pub(crate) value_type: Option<Type>,
    pub(crate) value: Expression,
    pub(crate) location: Location,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SelfObject {
    pub(crate) resolved_type: types::TypeRef,
    pub(crate) location: Location,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct True {
    pub(crate) resolved_type: types::TypeRef,
    pub(crate) location: Location,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Nil {
    pub(crate) resolved_type: types::TypeRef,
    pub(crate) location: Location,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct False {
    pub(crate) resolved_type: types::TypeRef,
    pub(crate) location: Location,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Next {
    pub(crate) location: Location,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Break {
    pub(crate) location: Location,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Ref {
    pub(crate) resolved_type: types::TypeRef,
    pub(crate) value: Expression,
    pub(crate) location: Location,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Mut {
    pub(crate) pointer_to_method: Option<types::MethodId>,
    pub(crate) resolved_type: types::TypeRef,
    pub(crate) value: Expression,
    pub(crate) location: Location,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Recover {
    pub(crate) resolved_type: types::TypeRef,
    pub(crate) body: Vec<Expression>,
    pub(crate) location: Location,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct And {
    pub(crate) resolved_type: types::TypeRef,
    pub(crate) left: Expression,
    pub(crate) right: Expression,
    pub(crate) location: Location,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Or {
    pub(crate) resolved_type: types::TypeRef,
    pub(crate) left: Expression,
    pub(crate) right: Expression,
    pub(crate) location: Location,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TypeCast {
    pub(crate) resolved_type: types::TypeRef,
    pub(crate) value: Expression,
    pub(crate) cast_to: Type,
    pub(crate) location: Location,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Throw {
    pub(crate) resolved_type: types::TypeRef,
    pub(crate) return_type: types::TypeRef,
    pub(crate) value: Expression,
    pub(crate) location: Location,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Return {
    pub(crate) resolved_type: types::TypeRef,
    pub(crate) value: Option<Expression>,
    pub(crate) location: Location,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TuplePattern {
    pub(crate) field_ids: Vec<types::FieldId>,
    pub(crate) values: Vec<Pattern>,
    pub(crate) location: Location,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FieldPattern {
    pub(crate) field_id: Option<types::FieldId>,
    pub(crate) field: Field,
    pub(crate) pattern: Pattern,
    pub(crate) location: Location,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TypePattern {
    pub(crate) type_id: Option<types::TypeId>,
    pub(crate) values: Vec<FieldPattern>,
    pub(crate) location: Location,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ConstructorPattern {
    pub(crate) constructor_id: Option<types::ConstructorId>,
    pub(crate) name: Constant,
    pub(crate) values: Vec<Pattern>,
    pub(crate) location: Location,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct WildcardPattern {
    pub(crate) location: Location,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct IdentifierPattern {
    pub(crate) variable_id: Option<types::VariableId>,
    pub(crate) name: Identifier,
    pub(crate) mutable: bool,
    pub(crate) value_type: Option<Type>,
    pub(crate) location: Location,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ConstantPattern {
    pub(crate) kind: types::ConstantPatternKind,
    pub(crate) source: Option<Identifier>,
    pub(crate) name: String,
    pub(crate) location: Location,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct OrPattern {
    pub(crate) patterns: Vec<Pattern>,
    pub(crate) location: Location,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StringPattern {
    pub value: String,
    pub location: Location,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Pattern {
    Type(Box<TypePattern>),
    Constant(Box<ConstantPattern>),
    Identifier(Box<IdentifierPattern>),
    Int(Box<IntLiteral>),
    String(Box<StringPattern>),
    Tuple(Box<TuplePattern>),
    Constructor(Box<ConstructorPattern>),
    Wildcard(Box<WildcardPattern>),
    True(Box<True>),
    False(Box<False>),
    Or(Box<OrPattern>),
}

impl Pattern {
    pub(crate) fn location(&self) -> Location {
        match self {
            Pattern::Constant(ref n) => n.location,
            Pattern::Constructor(ref n) => n.location,
            Pattern::Int(ref n) => n.location,
            Pattern::String(ref n) => n.location,
            Pattern::Identifier(ref n) => n.location,
            Pattern::Tuple(ref n) => n.location,
            Pattern::Type(ref n) => n.location,
            Pattern::Wildcard(ref n) => n.location,
            Pattern::True(ref n) => n.location,
            Pattern::False(ref n) => n.location,
            Pattern::Or(ref n) => n.location,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MatchCase {
    pub(crate) variable_ids: Vec<types::VariableId>,
    pub(crate) pattern: Pattern,
    pub(crate) guard: Option<Expression>,
    pub(crate) body: Vec<Expression>,
    pub(crate) location: Location,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Match {
    pub(crate) resolved_type: types::TypeRef,
    pub(crate) expression: Expression,
    pub(crate) cases: Vec<MatchCase>,
    pub(crate) location: Location,
    pub(crate) write_result: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Loop {
    pub(crate) body: Vec<Expression>,
    pub(crate) location: Location,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Module {
    pub(crate) documentation: String,
    pub(crate) module_id: types::ModuleId,
    pub(crate) expressions: Vec<TopLevelExpression>,
    pub(crate) location: Location,
}

/// A compiler pass for lowering ASTs to HIR modules.
pub(crate) struct LowerToHir<'a> {
    state: &'a mut State,
    module: types::ModuleId,
}

impl<'a> LowerToHir<'a> {
    pub(crate) fn run_all(
        state: &mut State,
        modules: Vec<ParsedModule>,
    ) -> Vec<Module> {
        modules
            .into_iter()
            .map(|module| {
                let module_id = types::Module::alloc(
                    &mut state.db,
                    module.name.clone(),
                    module.ast.file.clone(),
                );

                LowerToHir { state, module: module_id }.run(module)
            })
            .collect()
    }

    fn run(mut self, module: ParsedModule) -> Module {
        let (doc, expressions) =
            self.top_level_expressions(module.ast.expressions);
        let location = module.ast.location;

        Module {
            documentation: doc,
            module_id: self.module,
            expressions,
            location,
        }
    }

    fn file(&self) -> PathBuf {
        self.module.file(&self.state.db)
    }

    fn top_level_expressions(
        &mut self,
        nodes: Vec<ast::TopLevelExpression>,
    ) -> (String, Vec<TopLevelExpression>) {
        let mut nodes = nodes.into_iter().peekable();
        let mut last_line = 0;
        let mut doc = String::new();

        while let Some(ast::TopLevelExpression::Comment(c)) = nodes.peek() {
            let line = c.location.line_start;

            if line - last_line == 1 {
                if !doc.is_empty() {
                    doc.push('\n');
                }

                doc.push_str(&c.value);
                nodes.next();
                last_line = line;
            } else {
                break;
            }
        }

        let mut exprs = Vec::new();
        let mut comments = Comments::new();

        for node in nodes {
            match node {
                ast::TopLevelExpression::DefineConstant(node) => {
                    let doc = comments.documentation_for(&node.location);

                    exprs.push(self.define_constant(*node, doc));
                }
                ast::TopLevelExpression::DefineMethod(node) => {
                    let doc = comments.documentation_for(&node.location);

                    exprs.push(self.define_module_method(*node, doc));
                }
                ast::TopLevelExpression::DefineType(node) => {
                    let doc = comments.documentation_for(&node.location);

                    exprs.push(self.define_type(*node, doc));
                }
                ast::TopLevelExpression::DefineTrait(node) => {
                    let doc = comments.documentation_for(&node.location);

                    exprs.push(self.define_trait(*node, doc));
                }
                ast::TopLevelExpression::ReopenType(node) => {
                    exprs.push(self.reopen_type(*node));
                }
                ast::TopLevelExpression::ImplementTrait(node) => {
                    exprs.push(self.implement_trait(*node));
                }
                ast::TopLevelExpression::Import(node) => {
                    // Build tags are evaluated as modules are parsed and
                    // imports are crawled. We ignore any imports filtered out
                    // through those tags here, such that the rest of the
                    // compilation pipeline doesn't need to care about them.
                    if node.include {
                        exprs.push(self.import(*node));
                    }
                }
                ast::TopLevelExpression::ExternImport(node) => {
                    exprs.push(self.extern_import(*node));
                }
                ast::TopLevelExpression::Comment(c) => {
                    comments.push(*c);
                }
            }
        }

        (doc, exprs)
    }

    fn define_constant(
        &mut self,
        node: ast::DefineConstant,
        documentation: String,
    ) -> TopLevelExpression {
        let node = DefineConstant {
            documentation,
            public: node.public,
            constant_id: None,
            name: Constant {
                name: node.name.name,
                location: node.name.location,
            },
            value: self.const_value(node.value),
            location: node.location,
        };

        TopLevelExpression::Constant(Box::new(node))
    }

    fn define_module_method(
        &mut self,
        node: ast::DefineMethod,
        documentation: String,
    ) -> TopLevelExpression {
        self.operator_method_not_allowed(node.operator, node.location);

        let external = matches!(node.kind, ast::MethodKind::Extern);

        if external && node.body.is_none() {
            TopLevelExpression::ExternFunction(Box::new(DefineExternFunction {
                documentation,
                public: node.public,
                name: self.identifier(node.name),
                variadic: node.arguments.as_ref().map_or(false, |a| a.variadic),
                arguments: self.optional_method_arguments(node.arguments),
                return_type: node.return_type.map(|n| self.type_reference(n)),
                method_id: None,
                location: node.location,
            }))
        } else {
            TopLevelExpression::ModuleMethod(Box::new(DefineModuleMethod {
                inline: node.inline,
                documentation,
                public: node.public,
                c_calling_convention: external,
                name: self.identifier(node.name),
                type_parameters: self
                    .optional_type_parameters(node.type_parameters),
                arguments: self.optional_method_arguments(node.arguments),
                return_type: node.return_type.map(|n| self.type_reference(n)),
                body: self.optional_expressions(node.body),
                method_id: None,
                location: node.location,
            }))
        }
    }

    fn optional_method_arguments(
        &self,
        node: Option<ast::MethodArguments>,
    ) -> Vec<MethodArgument> {
        if let Some(types) = node {
            types.values.into_iter().map(|n| self.method_argument(n)).collect()
        } else {
            Vec::new()
        }
    }

    fn method_argument(&self, node: ast::MethodArgument) -> MethodArgument {
        MethodArgument {
            name: self.identifier(node.name),
            value_type: self.type_reference(node.value_type),
            location: node.location,
        }
    }

    fn define_type(
        &mut self,
        node: ast::DefineType,
        documentation: String,
    ) -> TopLevelExpression {
        if !matches!(node.semantics, ast::TypeSemantics::Default) {
            match node.kind {
                ast::TypeKind::Enum | ast::TypeKind::Regular => {}
                _ => {
                    self.state.diagnostics.error(
                        DiagnosticId::InvalidType,
                        "only regular and 'enum' types support the 'inline' \
                        and 'copy' keywords",
                        self.file(),
                        node.name.location,
                    );
                }
            }
        }

        if let ast::TypeKind::Extern = node.kind {
            return self.define_extern_type(node, documentation);
        }

        TopLevelExpression::Type(Box::new(DefineType {
            documentation,
            public: node.public,
            semantics: match node.semantics {
                ast::TypeSemantics::Default => TypeSemantics::Default,
                ast::TypeSemantics::Inline => TypeSemantics::Inline,
                ast::TypeSemantics::Copy => TypeSemantics::Copy,
            },
            type_id: None,
            kind: match node.kind {
                ast::TypeKind::Async => TypeKind::Async,
                ast::TypeKind::Enum => TypeKind::Enum,
                ast::TypeKind::Builtin => TypeKind::Builtin,
                _ => TypeKind::Regular,
            },
            name: self.constant(node.name),
            type_parameters: self
                .optional_type_parameters(node.type_parameters),
            body: self.type_expressions(node.body),
            location: node.location,
        }))
    }

    fn define_extern_type(
        &mut self,
        node: ast::DefineType,
        documentation: String,
    ) -> TopLevelExpression {
        let mut fields = Vec::new();
        let mut comments = Comments::new();

        for expr in node.body.values {
            match expr {
                ast::TypeExpression::DefineField(n) => {
                    let doc = comments.documentation_for(&n.location);

                    fields.push(self.define_field(*n, doc));
                }
                ast::TypeExpression::Comment(c) => {
                    comments.push(*c);
                }
                _ => unreachable!(),
            }
        }

        TopLevelExpression::ExternType(Box::new(DefineExternType {
            documentation,
            public: node.public,
            type_id: None,
            name: self.constant(node.name),
            fields,
            location: node.location,
        }))
    }

    fn type_expressions(
        &mut self,
        node: ast::TypeExpressions,
    ) -> Vec<TypeExpression> {
        let mut exprs = Vec::new();
        let mut comments = Comments::new();

        for n in node.values {
            match n {
                ast::TypeExpression::DefineMethod(node) => {
                    let doc = comments.documentation_for(&node.location);

                    exprs.push(self.define_method_in_type(*node, doc));
                }
                ast::TypeExpression::DefineField(node) => {
                    let doc = comments.documentation_for(&node.location);

                    exprs.push(TypeExpression::Field(Box::new(
                        self.define_field(*node, doc),
                    )));
                }
                ast::TypeExpression::DefineConstructor(node) => {
                    let doc = comments.documentation_for(&node.location);

                    exprs.push(self.define_case(*node, doc));
                }
                ast::TypeExpression::Comment(c) => {
                    comments.push(*c);
                }
            }
        }

        exprs
    }

    fn define_field(
        &self,
        node: ast::DefineField,
        documentation: String,
    ) -> DefineField {
        DefineField {
            documentation,
            public: node.public,
            mutable: node.mutable,
            field_id: None,
            name: self.identifier(node.name),
            value_type: self.type_reference(node.value_type),
            location: node.location,
        }
    }

    fn define_case(
        &mut self,
        node: ast::DefineConstructor,
        documentation: String,
    ) -> TypeExpression {
        TypeExpression::Constructor(Box::new(DefineConstructor {
            documentation,
            method_id: None,
            constructor_id: None,
            name: self.constant(node.name),
            members: self.optional_types(node.members),
            location: node.location,
        }))
    }

    fn define_method_in_type(
        &mut self,
        node: ast::DefineMethod,
        documentation: String,
    ) -> TypeExpression {
        match node.kind {
            ast::MethodKind::Async | ast::MethodKind::AsyncMutable => {
                TypeExpression::AsyncMethod(
                    self.define_async_method(node, documentation),
                )
            }
            ast::MethodKind::Static => TypeExpression::StaticMethod(
                self.define_static_method(node, documentation),
            ),
            _ => TypeExpression::InstanceMethod(Box::new(
                self.define_instance_method(node, documentation),
            )),
        }
    }

    fn define_static_method(
        &mut self,
        node: ast::DefineMethod,
        documentation: String,
    ) -> Box<DefineStaticMethod> {
        self.operator_method_not_allowed(node.operator, node.location);

        Box::new(DefineStaticMethod {
            inline: node.inline,
            documentation,
            public: node.public,
            name: self.identifier(node.name),
            type_parameters: self
                .optional_type_parameters(node.type_parameters),
            arguments: self.optional_method_arguments(node.arguments),
            return_type: node.return_type.map(|n| self.type_reference(n)),
            body: self.optional_expressions(node.body),
            method_id: None,
            location: node.location,
        })
    }

    fn define_async_method(
        &mut self,
        node: ast::DefineMethod,
        documentation: String,
    ) -> Box<DefineAsyncMethod> {
        self.operator_method_not_allowed(node.operator, node.location);
        self.disallow_inline_method(&node);

        Box::new(DefineAsyncMethod {
            documentation,
            mutable: node.kind == ast::MethodKind::AsyncMutable,
            public: node.public,
            name: self.identifier(node.name),
            type_parameters: self
                .optional_type_parameters(node.type_parameters),
            arguments: self.optional_method_arguments(node.arguments),
            return_type: node.return_type.map(|n| self.type_reference(n)),
            body: self.optional_expressions(node.body),
            method_id: None,
            location: node.location,
        })
    }

    fn define_instance_method(
        &mut self,
        node: ast::DefineMethod,
        documentation: String,
    ) -> DefineInstanceMethod {
        DefineInstanceMethod {
            inline: node.inline,
            documentation,
            public: node.public,
            kind: match node.kind {
                ast::MethodKind::Moving => MethodKind::Moving,
                ast::MethodKind::Mutable => MethodKind::Mutable,
                _ => MethodKind::Regular,
            },
            name: self.identifier(node.name),
            type_parameters: self
                .optional_type_parameters(node.type_parameters),
            arguments: self.optional_method_arguments(node.arguments),
            return_type: node.return_type.map(|n| self.type_reference(n)),
            body: self.optional_expressions(node.body),
            method_id: None,
            location: node.location,
        }
    }

    fn define_required_method(
        &mut self,
        node: ast::DefineMethod,
        documentation: String,
    ) -> Box<DefineRequiredMethod> {
        self.disallow_inline_method(&node);

        Box::new(DefineRequiredMethod {
            documentation,
            public: node.public,
            kind: match node.kind {
                ast::MethodKind::Moving => MethodKind::Moving,
                ast::MethodKind::Mutable => MethodKind::Mutable,
                _ => MethodKind::Regular,
            },
            name: self.identifier(node.name),
            type_parameters: self
                .optional_type_parameters(node.type_parameters),
            arguments: self.optional_method_arguments(node.arguments),
            return_type: node.return_type.map(|n| self.type_reference(n)),
            method_id: None,
            location: node.location,
        })
    }

    fn optional_type_bounds(
        &mut self,
        node: Option<ast::TypeBounds>,
    ) -> Vec<TypeBound> {
        if let Some(types) = node {
            types.values.into_iter().map(|n| self.type_bound(n)).collect()
        } else {
            Vec::new()
        }
    }

    fn type_bound(&mut self, node: ast::TypeBound) -> TypeBound {
        let name = self.constant(node.name);
        let (reqs, mutable, copy) = self.define_type_parameter_requirements(
            &name.name,
            node.requirements.values,
        );

        TypeBound {
            name,
            requirements: reqs,
            mutable,
            copy,
            location: node.location,
        }
    }

    fn define_type_parameter_requirements(
        &mut self,
        name: &str,
        nodes: Vec<ast::Requirement>,
    ) -> (Vec<TypeName>, bool, bool) {
        let mut mutable = false;
        let mut copy = false;
        let mut requirements = Vec::new();

        for req in nodes {
            match req {
                ast::Requirement::Trait(n) => {
                    requirements.push(self.type_name(n))
                }
                ast::Requirement::Mutable(loc) if mutable => {
                    let file = self.file();

                    self.state
                        .diagnostics
                        .duplicate_type_parameter_requirement(
                            name, "mut", file, loc,
                        );
                }
                ast::Requirement::Mutable(loc) if copy => {
                    self.state
                        .diagnostics
                        .mutable_copy_type_parameter(self.file(), loc);
                }
                ast::Requirement::Copy(loc) if copy => {
                    let file = self.file();

                    self.state
                        .diagnostics
                        .duplicate_type_parameter_requirement(
                            name, "copy", file, loc,
                        );
                }
                ast::Requirement::Copy(loc) if mutable => {
                    self.state
                        .diagnostics
                        .mutable_copy_type_parameter(self.file(), loc);
                }
                ast::Requirement::Mutable(_) => mutable = true,
                ast::Requirement::Copy(_) => copy = true,
            }
        }

        (requirements, mutable, copy)
    }

    fn define_trait(
        &mut self,
        node: ast::DefineTrait,
        documentation: String,
    ) -> TopLevelExpression {
        TopLevelExpression::Trait(Box::new(DefineTrait {
            documentation,
            public: node.public,
            trait_id: None,
            name: self.constant(node.name),
            type_parameters: self
                .optional_type_parameters(node.type_parameters),
            requirements: self.optional_type_names(node.requirements),
            body: self.trait_expressions(node.body),
            location: node.location,
        }))
    }

    fn trait_expressions(
        &mut self,
        node: ast::TraitExpressions,
    ) -> Vec<TraitExpression> {
        let mut exprs = Vec::new();
        let mut comments = Comments::new();

        for node in node.values {
            match node {
                ast::TraitExpression::DefineMethod(n) => {
                    let doc = comments.documentation_for(&n.location);

                    exprs.push(self.define_method_in_trait(*n, doc));
                }
                ast::TraitExpression::Comment(c) => {
                    comments.push(*c);
                }
            }
        }

        exprs
    }

    fn define_method_in_trait(
        &mut self,
        node: ast::DefineMethod,
        documentation: String,
    ) -> TraitExpression {
        if node.body.is_some() {
            TraitExpression::InstanceMethod(Box::new(
                self.define_instance_method(node, documentation),
            ))
        } else {
            TraitExpression::RequiredMethod(
                self.define_required_method(node, documentation),
            )
        }
    }

    fn reopen_type(&mut self, node: ast::ReopenType) -> TopLevelExpression {
        TopLevelExpression::Reopen(Box::new(ReopenType {
            type_id: None,
            type_name: self.constant(node.type_name),
            body: self.reopen_type_expressions(node.body),
            bounds: self.optional_type_bounds(node.bounds),
            location: node.location,
        }))
    }

    fn reopen_type_expressions(
        &mut self,
        nodes: ast::ImplementationExpressions,
    ) -> Vec<ReopenTypeExpression> {
        let mut exprs = Vec::new();
        let mut comments = Comments::new();

        for node in nodes.values {
            match node {
                ast::ImplementationExpression::DefineMethod(n) => {
                    let doc = comments.documentation_for(&n.location);

                    exprs.push(self.define_method_in_reopen_type(*n, doc));
                }
                ast::ImplementationExpression::Comment(c) => {
                    comments.push(*c);
                }
            }
        }

        exprs
    }

    fn define_method_in_reopen_type(
        &mut self,
        node: ast::DefineMethod,
        documentation: String,
    ) -> ReopenTypeExpression {
        match node.kind {
            ast::MethodKind::Static => ReopenTypeExpression::StaticMethod(
                self.define_static_method(node, documentation),
            ),
            ast::MethodKind::Async | ast::MethodKind::AsyncMutable => {
                ReopenTypeExpression::AsyncMethod(
                    self.define_async_method(node, documentation),
                )
            }
            _ => ReopenTypeExpression::InstanceMethod(Box::new(
                self.define_instance_method(node, documentation),
            )),
        }
    }

    fn implement_trait(
        &mut self,
        node: ast::ImplementTrait,
    ) -> TopLevelExpression {
        TopLevelExpression::Implement(Box::new(ImplementTrait {
            trait_name: self.type_name(node.trait_name),
            type_name: self.constant(node.type_name),
            bounds: self.optional_type_bounds(node.bounds),
            body: self.trait_implementation_expressions(node.body),
            location: node.location,
            trait_instance: None,
            type_instance: None,
        }))
    }

    fn trait_implementation_expressions(
        &mut self,
        node: ast::ImplementationExpressions,
    ) -> Vec<DefineInstanceMethod> {
        let mut exprs = Vec::new();
        let mut comments = Comments::new();

        for node in node.values {
            match node {
                ast::ImplementationExpression::DefineMethod(n) => {
                    let doc = comments.documentation_for(&n.location);

                    exprs.push(self.define_instance_method(*n, doc));
                }
                ast::ImplementationExpression::Comment(c) => {
                    comments.push(*c);
                }
            }
        }

        exprs
    }

    fn import(&self, node: ast::Import) -> TopLevelExpression {
        TopLevelExpression::Import(Box::new(Import {
            source: self.import_module_path(node.path),
            symbols: self.import_symbols(node.symbols),
            location: node.location,
        }))
    }

    fn extern_import(&self, node: ast::ExternImport) -> TopLevelExpression {
        TopLevelExpression::ExternImport(Box::new(ExternImport {
            source: node.path.path,
            location: node.location,
        }))
    }

    fn import_module_path(&self, node: ast::ImportPath) -> Vec<Identifier> {
        node.steps.into_iter().map(|n| self.identifier(n)).collect()
    }

    fn import_symbols(
        &self,
        node: Option<ast::ImportSymbols>,
    ) -> Vec<ImportSymbol> {
        let mut values = Vec::new();

        if let Some(symbols) = node {
            for symbol in symbols.values {
                let name =
                    Identifier { name: symbol.name, location: symbol.location };

                let import_as = if let Some(n) = symbol.alias {
                    Identifier { name: n.name, location: n.location }
                } else {
                    name.clone()
                };

                let location =
                    Location::start_end(&name.location, &import_as.location);

                values.push(ImportSymbol { name, import_as, location });
            }
        }

        values
    }

    fn type_reference(&self, node: ast::Type) -> Type {
        match node {
            ast::Type::Named(node) => {
                Type::Named(Box::new(self.type_name(*node)))
            }
            ast::Type::Ref(node) => Type::Ref(self.reference_type(*node)),
            ast::Type::Mut(node) => Type::Mut(self.reference_type(*node)),
            ast::Type::Owned(node) => Type::Owned(self.reference_type(*node)),
            ast::Type::Uni(node) => Type::Uni(self.reference_type(*node)),
            ast::Type::Closure(node) => Type::Closure(self.closure_type(*node)),
            ast::Type::Tuple(node) => Type::Tuple(self.tuple_type(*node)),
        }
    }

    fn type_name(&self, node: ast::TypeName) -> TypeName {
        let source = self.optional_identifier(node.name.source);
        let name =
            Constant { name: node.name.name, location: node.name.location };
        let location = node.location;
        let arguments = if let Some(types) = node.arguments {
            types.values.into_iter().map(|n| self.type_reference(n)).collect()
        } else {
            Vec::new()
        };

        TypeName {
            source,
            resolved_type: types::TypeRef::Unknown,
            name,
            arguments,
            location,
            self_type: false,
        }
    }

    fn optional_identifier(
        &self,
        node: Option<ast::Identifier>,
    ) -> Option<Identifier> {
        node.map(|n| Identifier { name: n.name, location: n.location })
    }

    fn identifier(&self, node: ast::Identifier) -> Identifier {
        Identifier { name: node.name, location: node.location }
    }

    fn field(&self, node: ast::Field) -> Field {
        Field { name: node.name, location: node.location }
    }

    fn constant(&self, node: ast::Constant) -> Constant {
        Constant { name: node.name, location: node.location }
    }

    fn reference_type(&self, node: ast::ReferenceType) -> Box<ReferenceType> {
        Box::new(ReferenceType {
            type_reference: match node.type_reference {
                ast::ReferrableType::Named(node) => {
                    ReferrableType::Named(Box::new(self.type_name(*node)))
                }
                ast::ReferrableType::Closure(node) => {
                    ReferrableType::Closure(self.closure_type(*node))
                }
                ast::ReferrableType::Tuple(node) => {
                    ReferrableType::Tuple(self.tuple_type(*node))
                }
            },
            location: node.location,
        })
    }

    fn closure_type(&self, node: ast::ClosureType) -> Box<ClosureType> {
        Box::new(ClosureType {
            arguments: self.optional_types(node.arguments),
            return_type: node.return_type.map(|n| self.type_reference(n)),
            location: node.location,
            resolved_type: types::TypeRef::Unknown,
        })
    }

    fn tuple_type(&self, node: ast::TupleType) -> Box<TupleType> {
        Box::new(TupleType {
            resolved_type: types::TypeRef::Unknown,
            values: node
                .values
                .into_iter()
                .map(|n| self.type_reference(n))
                .collect(),
            location: node.location,
        })
    }

    fn optional_type_parameters(
        &mut self,
        node: Option<ast::TypeParameters>,
    ) -> Vec<TypeParameter> {
        if let Some(types) = node {
            types.values.into_iter().map(|n| self.type_parameter(n)).collect()
        } else {
            Vec::new()
        }
    }

    fn optional_types(&self, node: Option<ast::Types>) -> Vec<Type> {
        if let Some(types) = node {
            types.values.into_iter().map(|n| self.type_reference(n)).collect()
        } else {
            Vec::new()
        }
    }

    fn type_parameter(&mut self, node: ast::TypeParameter) -> TypeParameter {
        let name = self.constant(node.name);
        let location = node.location;
        let (reqs, mutable, copy) = if let Some(reqs) = node.requirements {
            self.define_type_parameter_requirements(&name.name, reqs.values)
        } else {
            (Vec::new(), false, false)
        };

        TypeParameter {
            type_parameter_id: None,
            name,
            requirements: reqs,
            location,
            mutable,
            copy,
        }
    }

    fn optional_type_names(
        &self,
        node: Option<ast::TypeNames>,
    ) -> Vec<TypeName> {
        if let Some(types) = node {
            self.type_names(types)
        } else {
            Vec::new()
        }
    }

    fn type_names(&self, node: ast::TypeNames) -> Vec<TypeName> {
        node.values.into_iter().map(|n| self.type_name(n)).collect()
    }

    fn const_value(&mut self, node: ast::Expression) -> ConstExpression {
        match node {
            ast::Expression::Int(node) => {
                ConstExpression::Int(Box::new(self.int_literal(*node)))
            }
            ast::Expression::Float(node) => {
                ConstExpression::Float(self.float_literal(*node))
            }
            ast::Expression::String(node) => self.const_string_literal(*node),
            ast::Expression::True(node) => {
                ConstExpression::True(self.true_literal(*node))
            }
            ast::Expression::False(node) => {
                ConstExpression::False(self.false_literal(*node))
            }
            ast::Expression::Binary(node) => {
                ConstExpression::Binary(self.const_binary(*node))
            }
            ast::Expression::Constant(node) => {
                ConstExpression::ConstantRef(self.constant_ref(*node))
            }
            ast::Expression::Group(node) => self.const_value(node.value),
            ast::Expression::Array(node) => {
                ConstExpression::Array(self.const_array(*node))
            }
            _ => unreachable!(),
        }
    }

    fn int_literal(&mut self, node: ast::IntLiteral) -> IntLiteral {
        let mut input = node.value;

        if input.contains('_') {
            input = input.replace('_', "");
        }

        let hex_prefix = if input.starts_with('-') { "-0x" } else { "0x" };
        let result = if let Some(slice) = input.strip_prefix(hex_prefix) {
            i64::from_str_radix(slice, 16).map(|v| {
                if input.starts_with('-') {
                    0_i64.wrapping_sub(v)
                } else {
                    v
                }
            })
        } else {
            i64::from_str(&input)
        };

        let value = match result {
            Ok(val) => val,
            Err(e) => {
                self.state.diagnostics.error(
                    DiagnosticId::InvalidSyntax,
                    format!("this Int literal is invalid: {}", e),
                    self.file(),
                    node.location,
                );

                0
            }
        };

        IntLiteral {
            value,
            resolved_type: types::TypeRef::Unknown,
            location: node.location,
        }
    }

    fn float_literal(&mut self, node: ast::FloatLiteral) -> Box<FloatLiteral> {
        let mut input = node.value;

        if input.contains('_') {
            input = input.replace('_', "");
        }

        let value = match f64::from_str(&input) {
            Ok(val) => val,
            Err(e) => {
                self.state.diagnostics.error(
                    DiagnosticId::InvalidSyntax,
                    format!("this Float literal is invalid: {}", e),
                    self.file(),
                    node.location,
                );

                0.0
            }
        };

        Box::new(FloatLiteral {
            value,
            resolved_type: types::TypeRef::Unknown,
            location: node.location,
        })
    }

    fn string_literal(&mut self, node: ast::StringLiteral) -> Expression {
        let mut text_node: Option<StringLiteral> = None;
        let mut args = Vec::new();

        // We concatenate consecutive Text and Escape nodes into a single
        // StringText node, such that when we lower to MIR we can avoid
        // unnecessary runtime concatenations.
        for n in node.values {
            let (val, loc) = match n {
                ast::StringValue::Text(n) => (n.value, n.location),
                ast::StringValue::Escape(n) => (n.value, n.location),
                ast::StringValue::Expression(node) => {
                    if let Some(v) = text_node.take() {
                        args.push(Expression::String(Box::new(v)));
                    }

                    let rec = self.expression(node.value);
                    let loc = rec.location();
                    let val = Expression::call(
                        rec,
                        TO_STRING_METHOD,
                        Vec::new(),
                        loc,
                    );

                    args.push(val);
                    continue;
                }
            };

            if let Some(text) = text_node.as_mut() {
                text.value.push_str(&val);
                text.location = Location::start_end(&text.location, &loc);
            } else {
                text_node = Some(StringLiteral {
                    value: val,
                    location: loc,
                    resolved_type: types::TypeRef::Unknown,
                });
            }
        }

        if let Some(v) = text_node {
            args.push(Expression::String(Box::new(v)));
        }

        match args.len() {
            0 => Expression::string(String::new(), node.location),
            1 => {
                let mut expr = args.pop().unwrap();

                if let Expression::String(v) = &mut expr {
                    v.location = node.location;
                }

                expr
            }
            _ => {
                // let buf = StringBuffer.with_capacity(...)
                let mut body = vec![Expression::define_variable(
                    STR_BUF_VAR,
                    Expression::call_static(
                        STRING_BUFFER_INTERNAL_NAME,
                        STRING_BUFFER_NEW,
                        Vec::new(),
                        node.location,
                    ),
                    node.location,
                )];
                let var_ref =
                    Expression::identifier_ref(STR_BUF_VAR, node.location);

                for arg in args {
                    let loc = arg.location();

                    // buf.push(...)
                    body.push(Expression::call(
                        var_ref.clone(),
                        STRING_BUFFER_PUSH,
                        vec![arg],
                        loc,
                    ));
                }

                // buf.into_string
                body.push(Expression::call(
                    var_ref,
                    STRING_BUFFER_INTO_STRING,
                    Vec::new(),
                    node.location,
                ));

                Expression::Scope(Box::new(Scope {
                    resolved_type: types::TypeRef::Unknown,
                    body,
                    location: node.location,
                }))
            }
        }
    }

    fn array_literal(&mut self, node: ast::Array) -> Expression {
        let var_ref = Expression::identifier_ref(ARRAY_LIT_VAR, node.location);
        let mut pushes = Vec::new();

        for n in node.values {
            if let ast::Expression::Comment(_) = n {
                continue;
            }

            let arg = self.expression(n);
            let loc = arg.location();
            let push =
                Expression::call(var_ref.clone(), ARRAY_PUSH, vec![arg], loc);

            pushes.push(push);
        }

        let mut body = vec![Expression::define_variable(
            ARRAY_LIT_VAR,
            Expression::call_static(
                ARRAY_INTERNAL_NAME,
                ARRAY_WITH_CAPACITY,
                vec![Expression::Int(Box::new(IntLiteral {
                    value: pushes.len() as _,
                    resolved_type: types::TypeRef::Unknown,
                    location: node.location,
                }))],
                node.location,
            ),
            node.location,
        )];

        body.append(&mut pushes);
        body.push(var_ref);

        Expression::Scope(Box::new(Scope {
            resolved_type: types::TypeRef::Unknown,
            body,
            location: node.location,
        }))
    }

    fn tuple_literal(&mut self, node: ast::Tuple) -> Box<TupleLiteral> {
        Box::new(TupleLiteral {
            type_id: None,
            value_types: Vec::new(),
            resolved_type: types::TypeRef::Unknown,
            values: self.values(node.values),
            location: node.location,
        })
    }

    fn const_string_literal(
        &mut self,
        node: ast::StringLiteral,
    ) -> ConstExpression {
        let mut value = String::new();

        // While we could in theory support string interpolation, for the sake
        // of simplicity we don't. This ensures we don't have to main two
        // versions of string conversion for constant types: one in the standard
        // library, and one here in the compiler.
        for val in node.values {
            match val {
                ast::StringValue::Text(n) => value += &n.value,
                ast::StringValue::Escape(n) => value += &n.value,
                ast::StringValue::Expression(_) => unreachable!(),
            }
        }

        ConstExpression::String(Box::new(ConstStringLiteral {
            value,
            resolved_type: types::TypeRef::Unknown,
            location: node.location,
        }))
    }

    fn const_binary(&mut self, node: ast::Binary) -> Box<ConstBinary> {
        let left = self.const_value(node.left);
        let right = self.const_value(node.right);
        let location = node.location;
        let resolved_type = types::TypeRef::Unknown;
        let operator = Operator::from_ast(node.operator.kind);

        Box::new(ConstBinary { left, right, operator, resolved_type, location })
    }

    fn const_array(&mut self, node: ast::Array) -> Box<ConstArray> {
        let mut values = Vec::new();

        for expr in node.values {
            if let ast::Expression::Comment(_) = expr {
                continue;
            }

            values.push(self.const_value(expr));
        }

        Box::new(ConstArray {
            resolved_type: types::TypeRef::Unknown,
            values,
            location: node.location,
        })
    }

    fn optional_expressions(
        &mut self,
        node: Option<ast::Expressions>,
    ) -> Vec<Expression> {
        if let Some(node) = node {
            self.expressions(node)
        } else {
            Vec::new()
        }
    }

    fn expressions(&mut self, node: ast::Expressions) -> Vec<Expression> {
        self.values(node.values)
    }

    fn values(&mut self, nodes: Vec<ast::Expression>) -> Vec<Expression> {
        nodes
            .into_iter()
            .filter_map(|n| {
                // Comments in sequences of values aren't useful in HIR, and
                // keeping them around somehow (e.g. by producing a Nil node)
                // may result in redundant unreachable code warnings, so we get
                // rid of comments here.
                if let ast::Expression::Comment(_) = n {
                    None
                } else {
                    Some(self.expression(n))
                }
            })
            .collect()
    }

    fn expression(&mut self, node: ast::Expression) -> Expression {
        match node {
            ast::Expression::Int(node) => {
                Expression::Int(Box::new(self.int_literal(*node)))
            }
            ast::Expression::String(node) => self.string_literal(*node),
            ast::Expression::Float(node) => {
                Expression::Float(self.float_literal(*node))
            }
            ast::Expression::Binary(node) => {
                Expression::Call(self.binary(*node))
            }
            ast::Expression::Field(node) => {
                Expression::FieldRef(self.field_ref(*node))
            }
            ast::Expression::Constant(node) => {
                Expression::ConstantRef(self.constant_ref(*node))
            }
            ast::Expression::Identifier(node) => {
                Expression::IdentifierRef(self.identifier_ref(*node))
            }
            ast::Expression::Call(node) => self.call(*node),
            ast::Expression::AssignVariable(node) => {
                Expression::AssignVariable(self.assign_variable(*node))
            }
            ast::Expression::ReplaceVariable(node) => {
                Expression::ReplaceVariable(self.replace_variable(*node))
            }
            ast::Expression::AssignField(node) => {
                Expression::AssignField(self.assign_field(*node))
            }
            ast::Expression::ReplaceField(node) => {
                Expression::ReplaceField(self.replace_field(*node))
            }
            ast::Expression::AssignSetter(node) => {
                Expression::AssignSetter(self.assign_setter(*node))
            }
            ast::Expression::ReplaceSetter(node) => {
                Expression::ReplaceSetter(self.replace_setter(*node))
            }
            ast::Expression::BinaryAssignVariable(node) => {
                Expression::AssignVariable(self.binary_assign_variable(*node))
            }
            ast::Expression::BinaryAssignField(node) => {
                Expression::AssignField(self.binary_assign_field(*node))
            }
            ast::Expression::BinaryAssignSetter(node) => {
                Expression::AssignSetter(self.binary_assign_setter(*node))
            }
            ast::Expression::Closure(node) => {
                Expression::Closure(self.closure(*node))
            }
            ast::Expression::DefineVariable(node) => {
                Expression::DefineVariable(self.define_variable(*node))
            }
            ast::Expression::SelfObject(node) => {
                Expression::SelfObject(self.self_keyword(*node))
            }
            ast::Expression::Group(node) => self.group(*node),
            ast::Expression::Next(node) => {
                Expression::Next(self.next_keyword(*node))
            }
            ast::Expression::Break(node) => {
                Expression::Break(self.break_keyword(*node))
            }
            ast::Expression::True(node) => {
                Expression::True(self.true_literal(*node))
            }
            ast::Expression::Nil(node) => {
                Expression::Nil(self.nil_literal(*node))
            }
            ast::Expression::False(node) => {
                Expression::False(self.false_literal(*node))
            }
            ast::Expression::Ref(node) => {
                Expression::Ref(self.reference(*node))
            }
            ast::Expression::Mut(node) => {
                Expression::Mut(self.mut_reference(*node))
            }
            ast::Expression::Recover(node) => {
                Expression::Recover(self.recover_expression(*node))
            }
            ast::Expression::And(node) => {
                Expression::And(self.and_expression(*node))
            }
            ast::Expression::Or(node) => {
                Expression::Or(self.or_expression(*node))
            }
            ast::Expression::TypeCast(node) => {
                Expression::TypeCast(self.type_cast(*node))
            }
            ast::Expression::Throw(node) => {
                Expression::Throw(self.throw_expression(*node))
            }
            ast::Expression::Return(node) => {
                Expression::Return(self.return_expression(*node))
            }
            ast::Expression::Try(node) => self.try_expression(*node),
            ast::Expression::If(node) => {
                Expression::Match(self.if_expression(*node))
            }
            ast::Expression::Loop(node) => {
                Expression::Loop(self.loop_expression(*node))
            }
            ast::Expression::While(node) => {
                Expression::Loop(self.while_expression(*node))
            }
            ast::Expression::For(node) => {
                Expression::Scope(self.for_expression(*node))
            }
            ast::Expression::Scope(node) => {
                Expression::Scope(self.scope(*node))
            }
            ast::Expression::Match(node) => {
                Expression::Match(self.match_expression(*node))
            }
            ast::Expression::Array(node) => self.array_literal(*node),
            ast::Expression::Tuple(node) => {
                Expression::Tuple(self.tuple_literal(*node))
            }
            ast::Expression::Comment(c) => Expression::Nil(Box::new(Nil {
                resolved_type: types::TypeRef::Unknown,
                location: c.location,
            })),
        }
    }

    fn binary(&mut self, node: ast::Binary) -> Box<Call> {
        let op = Operator::from_ast(node.operator.kind);

        Box::new(Call {
            kind: types::CallKind::Unknown,
            receiver: Some(self.expression(node.left)),
            name: Identifier {
                name: op.method_name().to_string(),
                location: node.operator.location,
            },
            parens: true,
            in_mut: false,
            usage: Usage::Used,
            arguments: vec![Argument::Positional(Box::new(
                PositionalArgument {
                    value: self.expression(node.right),
                    expected_type: types::TypeRef::Unknown,
                },
            ))],
            location: node.location,
        })
    }

    fn field_ref(&self, node: ast::Field) -> Box<FieldRef> {
        Box::new(FieldRef {
            info: None,
            name: node.name,
            in_mut: false,
            location: node.location,
        })
    }

    fn constant_ref(&self, node: ast::Constant) -> Box<ConstantRef> {
        Box::new(ConstantRef {
            kind: types::ConstantKind::Unknown,
            source: self.optional_identifier(node.source),
            name: node.name,
            resolved_type: types::TypeRef::Unknown,
            usage: Usage::Used,
            location: node.location,
        })
    }

    fn identifier_ref(&self, node: ast::Identifier) -> Box<IdentifierRef> {
        Box::new(IdentifierRef {
            kind: types::IdentifierKind::Unknown,
            name: node.name,
            usage: Usage::Used,
            location: node.location,
        })
    }

    fn call(&mut self, node: ast::Call) -> Expression {
        if self.is_builtin_call(&node) {
            if !self.module.is_std(&self.state.db) {
                self.state
                    .diagnostics
                    .intrinsic_not_available(self.file(), node.location);
            }

            // We special-case this instruction because we need to attach extra
            // type information, but don't want to introduce a dedicated
            // `size_of` keyword just for this.
            if node.name.name == "size_of_type_name" {
                return self.size_of(node);
            }

            return Expression::BuiltinCall(Box::new(BuiltinCall {
                info: None,
                name: self.identifier(node.name),
                arguments: self.optional_builtin_call_arguments(node.arguments),
                location: node.location,
            }));
        }

        Expression::Call(Box::new(Call {
            kind: types::CallKind::Unknown,
            receiver: node.receiver.map(|n| self.expression(n)),
            name: self.identifier(node.name),
            parens: node.arguments.is_some(),
            in_mut: false,
            usage: Usage::Used,
            arguments: self.optional_call_arguments(node.arguments),
            location: node.location,
        }))
    }

    fn size_of(&mut self, node: ast::Call) -> Expression {
        if let Some(ast::Argument::Positional(ast::Expression::Constant(n))) =
            node.arguments.and_then(|mut v| v.values.pop())
        {
            let argument = Type::Named(Box::new(TypeName {
                source: None,
                resolved_type: types::TypeRef::Unknown,
                name: Constant { name: n.name, location: n.location },
                arguments: Vec::new(),
                location: n.location,
                self_type: false,
            }));

            Expression::SizeOf(Box::new(SizeOf {
                argument,
                resolved_type: types::TypeRef::Unknown,
                location: node.location,
            }))
        } else {
            self.state.diagnostics.error(
                DiagnosticId::InvalidCall,
                "this builtin function call is invalid",
                self.file(),
                node.name.location,
            );

            Expression::Nil(Box::new(Nil {
                resolved_type: types::TypeRef::Unknown,
                location: node.location,
            }))
        }
    }

    fn optional_builtin_call_arguments(
        &mut self,
        arguments: Option<ast::Arguments>,
    ) -> Vec<Expression> {
        let mut exprs = Vec::new();

        if let Some(args) = arguments {
            for n in args.values.into_iter() {
                exprs.push(match n {
                    ast::Argument::Positional(n) => self.expression(n),
                    ast::Argument::Named(node) => {
                        self.state.diagnostics.error(
                            DiagnosticId::InvalidCall,
                            "builtin calls don't support named arguments",
                            self.file(),
                            node.name.location,
                        );

                        self.expression(node.value)
                    }
                });
            }
        }

        exprs
    }

    fn is_builtin_call(&self, node: &ast::Call) -> bool {
        if let Some(ast::Expression::Constant(ref node)) =
            node.receiver.as_ref()
        {
            node.name == BUILTIN_RECEIVER
        } else {
            false
        }
    }

    fn optional_call_arguments(
        &mut self,
        node: Option<ast::Arguments>,
    ) -> Vec<Argument> {
        if let Some(args) = node {
            args.values
                .into_iter()
                .map(|n| match n {
                    ast::Argument::Positional(node) => {
                        Argument::Positional(Box::new(PositionalArgument {
                            value: self.expression(node),
                            expected_type: types::TypeRef::Unknown,
                        }))
                    }
                    ast::Argument::Named(node) => {
                        Argument::Named(Box::new(NamedArgument {
                            index: 0,
                            name: self.identifier(node.name),
                            value: self.expression(node.value),
                            location: node.location,
                            expected_type: types::TypeRef::Unknown,
                        }))
                    }
                })
                .collect()
        } else {
            Vec::new()
        }
    }

    fn assign_variable(
        &mut self,
        node: ast::AssignVariable,
    ) -> Box<AssignVariable> {
        Box::new(AssignVariable {
            variable_id: None,
            variable: self.identifier(node.variable),
            value: self.expression(node.value),
            resolved_type: types::TypeRef::Unknown,
            location: node.location,
        })
    }

    fn replace_variable(
        &mut self,
        node: ast::ReplaceVariable,
    ) -> Box<ReplaceVariable> {
        Box::new(ReplaceVariable {
            variable_id: None,
            variable: self.identifier(node.variable),
            value: self.expression(node.value),
            resolved_type: types::TypeRef::Unknown,
            location: node.location,
        })
    }

    fn assign_field(&mut self, node: ast::AssignField) -> Box<AssignField> {
        Box::new(AssignField {
            field_id: None,
            field: self.field(node.field),
            value: self.expression(node.value),
            resolved_type: types::TypeRef::Unknown,
            location: node.location,
        })
    }

    fn replace_field(&mut self, node: ast::ReplaceField) -> Box<ReplaceField> {
        Box::new(ReplaceField {
            field_id: None,
            field: self.field(node.field),
            value: self.expression(node.value),
            resolved_type: types::TypeRef::Unknown,
            location: node.location,
        })
    }

    fn binary_assign_variable(
        &mut self,
        node: ast::BinaryAssignVariable,
    ) -> Box<AssignVariable> {
        let op = Operator::from_ast(node.operator.kind);
        let variable = self.identifier(node.variable);
        let receiver = Expression::IdentifierRef(Box::new(IdentifierRef {
            kind: types::IdentifierKind::Unknown,
            name: variable.name.clone(),
            usage: Usage::Used,
            location: variable.location,
        }));

        Box::new(AssignVariable {
            variable_id: None,
            variable,
            value: Expression::Call(Box::new(Call {
                kind: types::CallKind::Unknown,
                name: Identifier {
                    name: op.method_name().to_string(),
                    location: node.operator.location,
                },
                receiver: Some(receiver),
                parens: true,
                in_mut: false,
                usage: Usage::Used,
                arguments: vec![Argument::Positional(Box::new(
                    PositionalArgument {
                        value: self.expression(node.value),
                        expected_type: types::TypeRef::Unknown,
                    },
                ))],
                location: node.location,
            })),
            resolved_type: types::TypeRef::Unknown,
            location: node.location,
        })
    }

    fn binary_assign_field(
        &mut self,
        node: ast::BinaryAssignField,
    ) -> Box<AssignField> {
        let op = Operator::from_ast(node.operator.kind);
        let field = self.field(node.field);
        let receiver = Expression::FieldRef(Box::new(FieldRef {
            info: None,
            name: field.name.clone(),
            in_mut: false,
            location: field.location,
        }));

        Box::new(AssignField {
            field_id: None,
            field,
            value: Expression::Call(Box::new(Call {
                kind: types::CallKind::Unknown,
                name: Identifier {
                    name: op.method_name().to_string(),
                    location: node.operator.location,
                },
                receiver: Some(receiver),
                parens: true,
                in_mut: false,
                usage: Usage::Used,
                arguments: vec![Argument::Positional(Box::new(
                    PositionalArgument {
                        value: self.expression(node.value),
                        expected_type: types::TypeRef::Unknown,
                    },
                ))],
                location: node.location,
            })),
            resolved_type: types::TypeRef::Unknown,
            location: node.location,
        })
    }

    fn assign_setter(&mut self, node: ast::AssignSetter) -> Box<AssignSetter> {
        Box::new(AssignSetter {
            kind: types::CallKind::Unknown,
            receiver: self.expression(node.receiver),
            name: self.identifier(node.name),
            value: self.expression(node.value),
            location: node.location,
            usage: Usage::Used,
            expected_type: types::TypeRef::Unknown,
        })
    }

    fn replace_setter(
        &mut self,
        node: ast::ReplaceSetter,
    ) -> Box<ReplaceSetter> {
        Box::new(ReplaceSetter {
            field_id: None,
            resolved_type: types::TypeRef::Unknown,
            receiver: self.expression(node.receiver),
            name: self.identifier(node.name),
            value: self.expression(node.value),
            location: node.location,
        })
    }

    fn binary_assign_setter(
        &mut self,
        node: ast::BinaryAssignSetter,
    ) -> Box<AssignSetter> {
        let op = Operator::from_ast(node.operator.kind);
        let name = self.identifier(node.name);
        let setter_rec = self.expression(node.receiver);
        let getter_loc =
            Location::start_end(&setter_rec.location(), &name.location);
        let getter_rec = Expression::Call(Box::new(Call {
            kind: types::CallKind::Unknown,
            receiver: Some(setter_rec.clone()),
            name: name.clone(),
            parens: false,
            in_mut: false,
            usage: Usage::Used,
            arguments: Vec::new(),
            location: getter_loc,
        }));

        Box::new(AssignSetter {
            kind: types::CallKind::Unknown,
            receiver: setter_rec,
            name,
            value: Expression::Call(Box::new(Call {
                kind: types::CallKind::Unknown,
                receiver: Some(getter_rec),
                parens: true,
                in_mut: false,
                usage: Usage::Used,
                arguments: vec![Argument::Positional(Box::new(
                    PositionalArgument {
                        value: self.expression(node.value),
                        expected_type: types::TypeRef::Unknown,
                    },
                ))],
                name: Identifier {
                    name: op.method_name().to_string(),
                    location: node.operator.location,
                },
                location: node.location,
            })),
            location: node.location,
            usage: Usage::Used,
            expected_type: types::TypeRef::Unknown,
        })
    }

    fn closure(&mut self, node: ast::Closure) -> Box<Closure> {
        Box::new(Closure {
            closure_id: None,
            resolved_type: types::TypeRef::Unknown,
            moving: node.moving,
            arguments: self.optional_block_arguments(node.arguments),
            return_type: node.return_type.map(|n| self.type_reference(n)),
            body: self.expressions(node.body),
            location: node.location,
        })
    }

    fn optional_block_arguments(
        &self,
        node: Option<ast::BlockArguments>,
    ) -> Vec<BlockArgument> {
        if let Some(types) = node {
            types.values.into_iter().map(|n| self.block_argument(n)).collect()
        } else {
            Vec::new()
        }
    }

    fn block_argument(&self, node: ast::BlockArgument) -> BlockArgument {
        BlockArgument {
            variable_id: None,
            name: self.identifier(node.name),
            value_type: node.value_type.map(|n| self.type_reference(n)),
            location: node.location,
        }
    }

    fn define_variable(
        &mut self,
        node: ast::DefineVariable,
    ) -> Box<DefineVariable> {
        Box::new(DefineVariable {
            resolved_type: types::TypeRef::Unknown,
            mutable: node.mutable,
            variable_id: None,
            name: self.identifier(node.name),
            value_type: node.value_type.map(|v| self.type_reference(v)),
            value: self.expression(node.value),
            location: node.location,
        })
    }

    fn self_keyword(&self, node: ast::SelfObject) -> Box<SelfObject> {
        Box::new(SelfObject {
            resolved_type: types::TypeRef::Unknown,
            location: node.location,
        })
    }

    fn group(&mut self, node: ast::Group) -> Expression {
        self.expression(node.value)
    }

    fn next_keyword(&self, node: ast::Next) -> Box<Next> {
        Box::new(Next { location: node.location })
    }

    fn break_keyword(&self, node: ast::Break) -> Box<Break> {
        Box::new(Break { location: node.location })
    }

    fn true_literal(&self, node: ast::True) -> Box<True> {
        Box::new(True {
            resolved_type: types::TypeRef::Unknown,
            location: node.location,
        })
    }

    fn false_literal(&self, node: ast::False) -> Box<False> {
        Box::new(False {
            resolved_type: types::TypeRef::Unknown,
            location: node.location,
        })
    }

    fn nil_literal(&self, node: ast::Nil) -> Box<Nil> {
        Box::new(Nil {
            resolved_type: types::TypeRef::Unknown,
            location: node.location,
        })
    }

    fn reference(&mut self, node: ast::Ref) -> Box<Ref> {
        Box::new(Ref {
            resolved_type: types::TypeRef::Unknown,
            value: self.expression(node.value),
            location: node.location,
        })
    }

    fn mut_reference(&mut self, node: ast::Mut) -> Box<Mut> {
        let mut value = self.expression(node.value);

        match &mut value {
            Expression::Call(n) => n.in_mut = true,
            Expression::FieldRef(n) => n.in_mut = true,
            _ => {}
        }

        Box::new(Mut {
            pointer_to_method: None,
            resolved_type: types::TypeRef::Unknown,
            value,
            location: node.location,
        })
    }

    fn recover_expression(&mut self, node: ast::Recover) -> Box<Recover> {
        Box::new(Recover {
            resolved_type: types::TypeRef::Unknown,
            body: self.expressions(node.body),
            location: node.location,
        })
    }

    fn and_expression(&mut self, node: ast::And) -> Box<And> {
        Box::new(And {
            resolved_type: types::TypeRef::Unknown,
            left: self.expression(node.left),
            right: self.expression(node.right),
            location: node.location,
        })
    }

    fn or_expression(&mut self, node: ast::Or) -> Box<Or> {
        Box::new(Or {
            resolved_type: types::TypeRef::Unknown,
            left: self.expression(node.left),
            right: self.expression(node.right),
            location: node.location,
        })
    }

    fn type_cast(&mut self, node: ast::TypeCast) -> Box<TypeCast> {
        Box::new(TypeCast {
            resolved_type: types::TypeRef::Unknown,
            value: self.expression(node.value),
            cast_to: self.type_reference(node.cast_to),
            location: node.location,
        })
    }

    fn throw_expression(&mut self, node: ast::Throw) -> Box<Throw> {
        Box::new(Throw {
            resolved_type: types::TypeRef::Unknown,
            return_type: types::TypeRef::Unknown,
            value: self.expression(node.value),
            location: node.location,
        })
    }

    fn return_expression(&mut self, node: ast::Return) -> Box<Return> {
        let value = node.value.map(|n| self.expression(n));

        Box::new(Return {
            resolved_type: types::TypeRef::Unknown,
            value,
            location: node.location,
        })
    }

    fn try_expression(&mut self, node: ast::Try) -> Expression {
        Expression::Try(Box::new(Try {
            expression: self.expression(node.value),
            kind: types::ThrowKind::Unknown,
            location: node.location,
            return_type: types::TypeRef::Unknown,
        }))
    }

    fn if_expression(&mut self, node: ast::If) -> Box<Match> {
        let mut cases = vec![MatchCase {
            variable_ids: Vec::new(),
            pattern: Pattern::True(Box::new(True {
                resolved_type: types::TypeRef::Unknown,
                location: *node.if_true.condition.location(),
            })),
            guard: None,
            body: self.expressions(node.if_true.body),
            location: node.if_true.location,
        }];

        for cond in node.else_if {
            cases.push(MatchCase {
                variable_ids: Vec::new(),
                pattern: Pattern::Wildcard(Box::new(WildcardPattern {
                    location: cond.location,
                })),
                guard: Some(self.expression(cond.condition)),
                body: self.expressions(cond.body),
                location: cond.location,
            });
        }

        let mut has_else = false;

        if let Some(body) = node.else_body {
            let location = body.location;

            has_else = true;

            cases.push(MatchCase {
                variable_ids: Vec::new(),
                pattern: Pattern::Wildcard(Box::new(WildcardPattern {
                    location: body.location,
                })),
                guard: None,
                body: self.expressions(body),
                location,
            })
        } else {
            cases.push(MatchCase {
                variable_ids: Vec::new(),
                pattern: Pattern::Wildcard(Box::new(WildcardPattern {
                    location: node.location,
                })),
                guard: None,
                body: vec![Expression::Nil(Box::new(Nil {
                    resolved_type: types::TypeRef::Unknown,
                    location: node.location,
                }))],
                location: node.location,
            });
        }

        Box::new(Match {
            resolved_type: types::TypeRef::Unknown,
            expression: self.expression(node.if_true.condition),
            cases,
            location: node.location,
            write_result: has_else,
        })
    }

    fn loop_expression(&mut self, node: ast::Loop) -> Box<Loop> {
        Box::new(Loop {
            body: self.expressions(node.body),
            location: node.location,
        })
    }

    /// Desugars a `while` loop into a regular `loop`.
    ///
    /// Loops like this:
    ///
    ///     while x {
    ///       y
    ///     }
    ///
    /// Are desugared into this:
    ///
    ///     loop {
    ///       if x {
    ///         y
    ///       } else {
    ///         break
    ///       }
    ///     }
    fn while_expression(&mut self, node: ast::While) -> Box<Loop> {
        let location = *node.condition.location();
        let condition = self.expression(node.condition);
        let cond_body = self.expressions(node.body);
        let body = vec![Expression::Match(Box::new(Match {
            resolved_type: types::TypeRef::Unknown,
            expression: condition,
            cases: vec![
                MatchCase {
                    variable_ids: Vec::new(),
                    pattern: Pattern::True(Box::new(True {
                        resolved_type: types::TypeRef::Unknown,
                        location,
                    })),
                    guard: None,
                    body: cond_body,
                    location,
                },
                MatchCase {
                    variable_ids: Vec::new(),
                    pattern: Pattern::Wildcard(Box::new(WildcardPattern {
                        location,
                    })),
                    guard: None,
                    body: vec![self.break_expression(location)],
                    location: node.location,
                },
            ],
            location: node.location,
            write_result: true,
        }))];

        Box::new(Loop { body, location: node.location })
    }

    fn for_expression(&mut self, node: ast::For) -> Box<Scope> {
        let pat_loc = *node.pattern.location();
        let iter_loc = *node.iterator.location();
        let def_var = Expression::DefineVariable(Box::new(DefineVariable {
            resolved_type: types::TypeRef::Unknown,
            variable_id: None,
            mutable: false,
            name: Identifier { name: ITER_VAR.to_string(), location: pat_loc },
            value_type: None,
            value: Expression::Call(Box::new(Call {
                kind: types::CallKind::Unknown,
                receiver: Some(self.expression(node.iterator)),
                name: Identifier {
                    name: INTO_ITER_CALL.to_string(),
                    location: iter_loc,
                },
                arguments: Vec::new(),
                parens: false,
                in_mut: false,
                usage: Usage::Used,
                location: iter_loc,
            })),
            location: pat_loc,
        }));

        let loop_expr = Expression::Loop(Box::new(Loop {
            body: vec![Expression::Match(Box::new(Match {
                resolved_type: types::TypeRef::Unknown,
                // iter.next
                expression: Expression::Call(Box::new(Call {
                    kind: types::CallKind::Unknown,
                    receiver: Some(Expression::IdentifierRef(Box::new(
                        IdentifierRef {
                            name: ITER_VAR.to_string(),
                            kind: types::IdentifierKind::Unknown,
                            usage: Usage::Used,
                            location: iter_loc,
                        },
                    ))),
                    name: Identifier {
                        name: NEXT_CALL.to_string(),
                        location: iter_loc,
                    },
                    arguments: Vec::new(),
                    parens: false,
                    in_mut: false,
                    usage: Usage::Used,
                    location: iter_loc,
                })),
                cases: vec![
                    // case Some(...) -> body
                    MatchCase {
                        variable_ids: Vec::new(),
                        pattern: Pattern::Constructor(Box::new(
                            ConstructorPattern {
                                constructor_id: None,
                                name: Constant {
                                    name: SOME_CONS.to_string(),
                                    location: node.location,
                                },
                                values: vec![self.pattern(node.pattern)],
                                location: node.location,
                            },
                        )),
                        guard: None,
                        body: self.expressions(node.body),
                        location: pat_loc,
                    },
                    // case _ -> break
                    MatchCase {
                        variable_ids: Vec::new(),
                        pattern: Pattern::Wildcard(Box::new(WildcardPattern {
                            location: node.location,
                        })),
                        guard: None,
                        body: vec![Expression::Break(Box::new(Break {
                            location: node.location,
                        }))],
                        location: node.location,
                    },
                ],
                location: node.location,
                write_result: true,
            }))],
            location: node.location,
        }));

        Box::new(Scope {
            resolved_type: types::TypeRef::Unknown,
            body: vec![def_var, loop_expr],
            location: node.location,
        })
    }

    fn scope(&mut self, node: ast::Scope) -> Box<Scope> {
        Box::new(Scope {
            resolved_type: types::TypeRef::Unknown,
            body: self.expressions(node.body),
            location: node.location,
        })
    }

    fn match_expression(&mut self, node: ast::Match) -> Box<Match> {
        let mut cases = Vec::new();

        for node in node.expressions {
            if let ast::MatchExpression::Case(node) = node {
                cases.push(MatchCase {
                    variable_ids: Vec::new(),
                    pattern: self.pattern(node.pattern),
                    guard: node.guard.map(|n| self.expression(n)),
                    body: self.expressions(node.body),
                    location: node.location,
                });
            }
        }

        Box::new(Match {
            resolved_type: types::TypeRef::Unknown,
            expression: self.expression(node.expression),
            cases,
            location: node.location,
            write_result: true,
        })
    }

    fn pattern(&mut self, node: ast::Pattern) -> Pattern {
        match node {
            ast::Pattern::Constant(n) => {
                Pattern::Constant(Box::new(ConstantPattern {
                    kind: types::ConstantPatternKind::Unknown,
                    source: self.optional_identifier(n.source),
                    name: n.name,
                    location: n.location,
                }))
            }
            ast::Pattern::Identifier(n) => {
                Pattern::Identifier(Box::new(IdentifierPattern {
                    variable_id: None,
                    name: self.identifier(n.name),
                    mutable: n.mutable,
                    value_type: n.value_type.map(|n| self.type_reference(n)),
                    location: n.location,
                }))
            }
            ast::Pattern::Wildcard(n) => {
                Pattern::Wildcard(Box::new(WildcardPattern {
                    location: n.location,
                }))
            }
            ast::Pattern::Int(n) => {
                Pattern::Int(Box::new(self.int_literal(*n)))
            }
            ast::Pattern::True(n) => Pattern::True(self.true_literal(*n)),
            ast::Pattern::False(n) => Pattern::False(self.false_literal(*n)),
            ast::Pattern::Constructor(n) => {
                Pattern::Constructor(Box::new(ConstructorPattern {
                    constructor_id: None,
                    name: self.constant(n.name),
                    values: self.patterns(n.values),
                    location: n.location,
                }))
            }
            ast::Pattern::Tuple(n) => Pattern::Tuple(Box::new(TuplePattern {
                field_ids: Vec::new(),
                values: self.patterns(n.values),
                location: n.location,
            })),
            ast::Pattern::Type(n) => Pattern::Type(Box::new(TypePattern {
                type_id: None,
                values: n
                    .values
                    .into_iter()
                    .map(|n| FieldPattern {
                        field_id: None,
                        field: self.field(n.field),
                        pattern: self.pattern(n.pattern),
                        location: n.location,
                    })
                    .collect(),
                location: n.location,
            })),
            ast::Pattern::Or(n) => Pattern::Or(Box::new(OrPattern {
                patterns: self.patterns(n.patterns),
                location: n.location,
            })),
            ast::Pattern::String(n) => {
                let mut value = String::new();

                for val in n.values {
                    match val {
                        ast::StringValue::Text(n) => value += &n.value,
                        ast::StringValue::Escape(n) => value += &n.value,
                        ast::StringValue::Expression(_) => unreachable!(),
                    }
                }

                Pattern::String(Box::new(StringPattern {
                    value,
                    location: n.location,
                }))
            }
        }
    }

    fn patterns(&mut self, nodes: Vec<ast::Pattern>) -> Vec<Pattern> {
        nodes.into_iter().map(|n| self.pattern(n)).collect()
    }

    fn break_expression(&self, location: Location) -> Expression {
        Expression::Break(Box::new(Break { location }))
    }

    fn operator_method_not_allowed(
        &mut self,
        operator: bool,
        location: Location,
    ) {
        if !operator {
            return;
        }

        self.state.diagnostics.error(
            DiagnosticId::InvalidMethod,
            "operator methods must be regular instance methods",
            self.file(),
            location,
        );
    }

    fn disallow_inline_method(&mut self, node: &ast::DefineMethod) {
        if node.inline {
            self.state
                .diagnostics
                .invalid_inline_method(self.file(), node.location);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::test::{cols, loc};
    use ::ast::parser::Parser;
    use similar_asserts::assert_eq;
    use types::module_name::ModuleName;

    #[track_caller]
    fn parse(input: &str) -> ParsedModule {
        let name = ModuleName::new("std.foo");
        let ast = Parser::new(input.into(), "test.inko".into())
            .parse()
            .expect("failed to parse the module");

        ParsedModule { ast, name }
    }

    #[track_caller]
    fn parse_with_comments(input: &str) -> ParsedModule {
        let name = ModuleName::new("std.foo");
        let ast = Parser::with_comments(input.into(), "test.inko".into())
            .parse()
            .expect("failed to parse the module");

        ParsedModule { ast, name }
    }

    #[track_caller]
    fn lower(input: &str) -> (Module, usize) {
        let mut state = State::new(Config::new());
        let ast = parse(input);
        let mut hir = LowerToHir::run_all(&mut state, vec![ast]);

        (hir.pop().unwrap(), state.diagnostics.iter().count())
    }

    #[track_caller]
    fn lower_with_comments(input: &str) -> (Module, usize) {
        let mut state = State::new(Config::new());
        let ast = parse_with_comments(input);
        let mut hir = LowerToHir::run_all(&mut state, vec![ast]);

        (hir.pop().unwrap(), state.diagnostics.iter().count())
    }

    #[track_caller]
    fn lower_top_expr(input: &str) -> (TopLevelExpression, usize) {
        let (mut module, diags) = lower(input);

        (module.expressions.pop().unwrap(), diags)
    }

    #[track_caller]
    fn lower_type(input: &str) -> Type {
        let hir =
            lower(&format!("fn a(a: {}) {{}}", input)).0.expressions.remove(0);

        match hir {
            TopLevelExpression::ModuleMethod(mut node) => {
                node.arguments.remove(0).value_type
            }
            _ => {
                panic!("the top-level expression must be a module method")
            }
        }
    }

    #[track_caller]
    fn lower_expr(input: &str) -> (Expression, usize) {
        let (mut top, diags) = lower(input);
        let hir = top.expressions.remove(0);

        match hir {
            TopLevelExpression::ModuleMethod(mut node) => {
                (node.body.remove(0), diags)
            }
            _ => {
                panic!("the top-level expression must be a module method")
            }
        }
    }

    #[track_caller]
    fn lower_expr_with_comments(input: &str) -> (Expression, usize) {
        let (mut top, diags) = lower_with_comments(input);
        let hir = top.expressions.remove(0);

        match hir {
            TopLevelExpression::ModuleMethod(mut node) => {
                (node.body.remove(0), diags)
            }
            _ => {
                panic!("the top-level expression must be a module method")
            }
        }
    }

    #[test]
    fn test_lower_module() {
        let (hir, diags) = lower("  ");

        assert_eq!(diags, 0);
        assert_eq!(
            hir,
            Module {
                documentation: String::new(),
                module_id: types::ModuleId(0),
                expressions: Vec::new(),
                location: cols(1, 2)
            }
        );
    }

    #[test]
    fn test_lower_constant_with_single_string() {
        let (hir, diags) = lower_top_expr("let A = 'foo'");

        assert_eq!(diags, 0);
        assert_eq!(
            hir,
            TopLevelExpression::Constant(Box::new(DefineConstant {
                documentation: String::new(),
                public: false,
                constant_id: None,
                name: Constant { name: "A".to_string(), location: cols(5, 5) },
                value: ConstExpression::String(Box::new(ConstStringLiteral {
                    value: "foo".to_string(),
                    resolved_type: types::TypeRef::Unknown,
                    location: cols(9, 13)
                })),
                location: cols(1, 13)
            }))
        );
    }

    #[test]
    fn test_lower_public_constant() {
        let hir = lower_top_expr("let pub A = 'foo'").0;

        assert_eq!(
            hir,
            TopLevelExpression::Constant(Box::new(DefineConstant {
                documentation: String::new(),
                public: true,
                constant_id: None,
                name: Constant { name: "A".to_string(), location: cols(9, 9) },
                value: ConstExpression::String(Box::new(ConstStringLiteral {
                    value: "foo".to_string(),
                    resolved_type: types::TypeRef::Unknown,
                    location: cols(13, 17)
                })),
                location: cols(1, 17)
            }))
        );
    }

    #[test]
    fn test_lower_constant_with_double_string() {
        let (hir, diags) = lower_top_expr("let A = \"foo\"");

        assert_eq!(diags, 0);
        assert_eq!(
            hir,
            TopLevelExpression::Constant(Box::new(DefineConstant {
                documentation: String::new(),
                public: false,
                constant_id: None,
                name: Constant { name: "A".to_string(), location: cols(5, 5) },
                value: ConstExpression::String(Box::new(ConstStringLiteral {
                    value: "foo".to_string(),
                    resolved_type: types::TypeRef::Unknown,
                    location: cols(9, 13)
                })),
                location: cols(1, 13)
            })),
        );
    }

    #[test]
    fn test_lower_constant_with_int() {
        let (hir, diags) = lower_top_expr("let A = 10");

        assert_eq!(diags, 0);
        assert_eq!(
            hir,
            TopLevelExpression::Constant(Box::new(DefineConstant {
                documentation: String::new(),
                public: false,
                constant_id: None,
                name: Constant { name: "A".to_string(), location: cols(5, 5) },
                value: ConstExpression::Int(Box::new(IntLiteral {
                    value: 10,
                    resolved_type: types::TypeRef::Unknown,
                    location: cols(9, 10)
                })),
                location: cols(1, 10)
            }))
        );
    }

    #[test]
    fn test_lower_constant_with_float() {
        let (hir, diags) = lower_top_expr("let A = 10.2");

        assert_eq!(diags, 0);
        assert_eq!(
            hir,
            TopLevelExpression::Constant(Box::new(DefineConstant {
                documentation: String::new(),
                public: false,
                constant_id: None,
                name: Constant { name: "A".to_string(), location: cols(5, 5) },
                value: ConstExpression::Float(Box::new(FloatLiteral {
                    value: 10.2,
                    resolved_type: types::TypeRef::Unknown,
                    location: cols(9, 12)
                })),
                location: cols(1, 12)
            }))
        );
    }

    #[test]
    fn test_lower_constant_with_binary() {
        let (hir, diags) = lower_top_expr("let A = 10 + 2");

        assert_eq!(diags, 0);
        assert_eq!(
            hir,
            TopLevelExpression::Constant(Box::new(DefineConstant {
                documentation: String::new(),
                public: false,
                constant_id: None,
                name: Constant { name: "A".to_string(), location: cols(5, 5) },
                value: ConstExpression::Binary(Box::new(ConstBinary {
                    left: ConstExpression::Int(Box::new(IntLiteral {
                        value: 10,
                        resolved_type: types::TypeRef::Unknown,
                        location: cols(9, 10)
                    })),
                    right: ConstExpression::Int(Box::new(IntLiteral {
                        value: 2,
                        resolved_type: types::TypeRef::Unknown,
                        location: cols(14, 14)
                    })),
                    resolved_type: types::TypeRef::Unknown,
                    operator: Operator::Add,
                    location: cols(9, 14)
                })),
                location: cols(1, 14)
            }))
        );
    }

    #[test]
    fn test_lower_constant_with_array() {
        let (hir, diags) = lower_top_expr("let A = [10]");

        assert_eq!(diags, 0);
        assert_eq!(
            hir,
            TopLevelExpression::Constant(Box::new(DefineConstant {
                documentation: String::new(),
                public: false,
                constant_id: None,
                name: Constant { name: "A".to_string(), location: cols(5, 5) },
                value: ConstExpression::Array(Box::new(ConstArray {
                    resolved_type: types::TypeRef::Unknown,
                    values: vec![ConstExpression::Int(Box::new(IntLiteral {
                        value: 10,
                        resolved_type: types::TypeRef::Unknown,
                        location: cols(10, 11)
                    }))],
                    location: cols(9, 12)
                })),
                location: cols(1, 12)
            }))
        );
    }

    #[test]
    fn test_lower_constant_with_boolean_array() {
        let (hir, diags) = lower_top_expr("let A = [true, false]");

        assert_eq!(diags, 0);
        assert_eq!(
            hir,
            TopLevelExpression::Constant(Box::new(DefineConstant {
                documentation: String::new(),
                public: false,
                constant_id: None,
                name: Constant { name: "A".to_string(), location: cols(5, 5) },
                value: ConstExpression::Array(Box::new(ConstArray {
                    resolved_type: types::TypeRef::Unknown,
                    values: vec![
                        ConstExpression::True(Box::new(True {
                            resolved_type: types::TypeRef::Unknown,
                            location: cols(10, 13)
                        })),
                        ConstExpression::False(Box::new(False {
                            resolved_type: types::TypeRef::Unknown,
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
    fn test_lower_type_name() {
        let hir = lower_type("B[C]");

        assert_eq!(
            hir,
            Type::Named(Box::new(TypeName {
                source: None,
                resolved_type: types::TypeRef::Unknown,
                name: Constant { name: "B".to_string(), location: cols(9, 9) },
                arguments: vec![Type::Named(Box::new(TypeName {
                    source: None,
                    resolved_type: types::TypeRef::Unknown,
                    name: Constant {
                        name: "C".to_string(),
                        location: cols(11, 11)
                    },
                    arguments: Vec::new(),
                    location: cols(11, 11),
                    self_type: false,
                }))],
                location: cols(9, 12),
                self_type: false,
            }))
        );
    }

    #[test]
    fn test_lower_namespaced_type_name() {
        let hir = lower_type("a.B");

        assert_eq!(
            hir,
            Type::Named(Box::new(TypeName {
                self_type: false,
                source: Some(Identifier {
                    name: "a".to_string(),
                    location: cols(9, 9)
                }),
                resolved_type: types::TypeRef::Unknown,
                name: Constant {
                    name: "B".to_string(),
                    location: cols(11, 11)
                },
                arguments: Vec::new(),
                location: cols(9, 11)
            }))
        );
    }

    #[test]
    fn test_lower_reference_type() {
        let hir = lower_type("ref B");

        assert_eq!(
            hir,
            Type::Ref(Box::new(ReferenceType {
                type_reference: ReferrableType::Named(Box::new(TypeName {
                    self_type: false,
                    source: None,
                    resolved_type: types::TypeRef::Unknown,
                    name: Constant {
                        name: "B".to_string(),
                        location: cols(13, 13)
                    },
                    arguments: Vec::new(),
                    location: cols(13, 13)
                })),
                location: cols(9, 13)
            }))
        );
    }

    #[test]
    fn test_lower_mutable_reference_type() {
        let hir = lower_type("mut B");

        assert_eq!(
            hir,
            Type::Mut(Box::new(ReferenceType {
                type_reference: ReferrableType::Named(Box::new(TypeName {
                    self_type: false,
                    source: None,
                    resolved_type: types::TypeRef::Unknown,
                    name: Constant {
                        name: "B".to_string(),
                        location: cols(13, 13)
                    },
                    arguments: Vec::new(),
                    location: cols(13, 13)
                })),
                location: cols(9, 13)
            }))
        );
    }

    #[test]
    fn test_lower_uni_reference_type() {
        let hir = lower_type("uni B");

        assert_eq!(
            hir,
            Type::Uni(Box::new(ReferenceType {
                type_reference: ReferrableType::Named(Box::new(TypeName {
                    self_type: false,
                    source: None,
                    resolved_type: types::TypeRef::Unknown,
                    name: Constant {
                        name: "B".to_string(),
                        location: cols(13, 13)
                    },
                    arguments: Vec::new(),
                    location: cols(13, 13)
                })),
                location: cols(9, 13)
            }))
        );
    }

    #[test]
    fn test_lower_owned_reference_type() {
        let hir = lower_type("move B");

        assert_eq!(
            hir,
            Type::Owned(Box::new(ReferenceType {
                type_reference: ReferrableType::Named(Box::new(TypeName {
                    self_type: false,
                    source: None,
                    resolved_type: types::TypeRef::Unknown,
                    name: Constant {
                        name: "B".to_string(),
                        location: cols(14, 14)
                    },
                    arguments: Vec::new(),
                    location: cols(14, 14)
                })),
                location: cols(9, 14)
            }))
        );
    }

    #[test]
    fn test_lower_closure_type() {
        let hir = lower_type("fn (A) -> C");

        assert_eq!(
            hir,
            Type::Closure(Box::new(ClosureType {
                arguments: vec![Type::Named(Box::new(TypeName {
                    self_type: false,
                    source: None,
                    resolved_type: types::TypeRef::Unknown,
                    name: Constant {
                        name: "A".to_string(),
                        location: cols(13, 13)
                    },
                    arguments: Vec::new(),
                    location: cols(13, 13)
                }))],
                return_type: Some(Type::Named(Box::new(TypeName {
                    self_type: false,
                    source: None,
                    resolved_type: types::TypeRef::Unknown,
                    name: Constant {
                        name: "C".to_string(),
                        location: cols(19, 19)
                    },
                    arguments: Vec::new(),
                    location: cols(19, 19)
                }))),
                location: cols(9, 19),
                resolved_type: types::TypeRef::Unknown,
            }))
        );
    }

    #[test]
    fn test_lower_module_method() {
        let (hir, diags) = lower_top_expr("fn foo[A: X](a: B) -> D { 10 }");

        assert_eq!(diags, 0);
        assert_eq!(
            hir,
            TopLevelExpression::ModuleMethod(Box::new(DefineModuleMethod {
                inline: false,
                documentation: String::new(),
                public: false,
                c_calling_convention: false,
                name: Identifier {
                    name: "foo".to_string(),
                    location: cols(4, 6)
                },
                type_parameters: vec![TypeParameter {
                    type_parameter_id: None,
                    name: Constant {
                        name: "A".to_string(),
                        location: cols(8, 8)
                    },
                    requirements: vec![TypeName {
                        self_type: false,
                        source: None,
                        resolved_type: types::TypeRef::Unknown,
                        name: Constant {
                            name: "X".to_string(),
                            location: cols(11, 11)
                        },
                        arguments: Vec::new(),
                        location: cols(11, 11)
                    }],
                    mutable: false,
                    copy: false,
                    location: cols(8, 11)
                }],
                arguments: vec![MethodArgument {
                    name: Identifier {
                        name: "a".to_string(),
                        location: cols(14, 14)
                    },
                    value_type: Type::Named(Box::new(TypeName {
                        self_type: false,
                        source: None,
                        resolved_type: types::TypeRef::Unknown,
                        name: Constant {
                            name: "B".to_string(),
                            location: cols(17, 17)
                        },
                        arguments: Vec::new(),
                        location: cols(17, 17)
                    })),
                    location: cols(14, 17)
                }],
                return_type: Some(Type::Named(Box::new(TypeName {
                    self_type: false,
                    source: None,
                    resolved_type: types::TypeRef::Unknown,
                    name: Constant {
                        name: "D".to_string(),
                        location: cols(23, 23)
                    },
                    arguments: Vec::new(),
                    location: cols(23, 23)
                }))),
                body: vec![Expression::Int(Box::new(IntLiteral {
                    value: 10,
                    resolved_type: types::TypeRef::Unknown,
                    location: cols(27, 28)
                }))],
                method_id: None,
                location: cols(1, 30),
            })),
        );
    }

    #[test]
    fn test_lower_inline_module_method() {
        let (hir, diags) = lower_top_expr("fn inline foo {}");

        assert_eq!(diags, 0);
        assert_eq!(
            hir,
            TopLevelExpression::ModuleMethod(Box::new(DefineModuleMethod {
                inline: true,
                documentation: String::new(),
                public: false,
                c_calling_convention: false,
                name: Identifier {
                    name: "foo".to_string(),
                    location: cols(11, 13)
                },
                type_parameters: Vec::new(),
                arguments: Vec::new(),
                return_type: None,
                body: Vec::new(),
                method_id: None,
                location: cols(1, 16),
            })),
        );
    }

    #[test]
    fn test_lower_extern_function() {
        let (hir, diags) = lower_top_expr("fn extern foo");

        assert_eq!(diags, 0);
        assert_eq!(
            hir,
            TopLevelExpression::ExternFunction(Box::new(
                DefineExternFunction {
                    documentation: String::new(),
                    public: false,
                    name: Identifier {
                        name: "foo".to_string(),
                        location: cols(11, 13)
                    },
                    arguments: Vec::new(),
                    variadic: false,
                    return_type: None,
                    method_id: None,
                    location: cols(1, 13),
                }
            )),
        );
    }

    #[test]
    fn test_lower_extern_method_with_body() {
        let (hir, diags) = lower_top_expr("fn extern foo(a: A) -> B { 10 }");

        assert_eq!(diags, 0);
        assert_eq!(
            hir,
            TopLevelExpression::ModuleMethod(Box::new(DefineModuleMethod {
                inline: false,
                documentation: String::new(),
                public: false,
                c_calling_convention: true,
                name: Identifier {
                    name: "foo".to_string(),
                    location: cols(11, 13)
                },
                type_parameters: Vec::new(),
                arguments: vec![MethodArgument {
                    name: Identifier {
                        name: "a".to_string(),
                        location: cols(15, 15)
                    },
                    value_type: Type::Named(Box::new(TypeName {
                        self_type: false,
                        source: None,
                        resolved_type: types::TypeRef::Unknown,
                        name: Constant {
                            name: "A".to_string(),
                            location: cols(18, 18)
                        },
                        arguments: Vec::new(),
                        location: cols(18, 18)
                    })),
                    location: cols(15, 18)
                }],
                return_type: Some(Type::Named(Box::new(TypeName {
                    self_type: false,
                    source: None,
                    resolved_type: types::TypeRef::Unknown,
                    name: Constant {
                        name: "B".to_string(),
                        location: cols(24, 24)
                    },
                    arguments: Vec::new(),
                    location: cols(24, 24)
                }))),
                body: vec![Expression::Int(Box::new(IntLiteral {
                    value: 10,
                    resolved_type: types::TypeRef::Unknown,
                    location: cols(28, 29)
                }))],
                method_id: None,
                location: cols(1, 31),
            })),
        );
    }

    #[test]
    fn test_lower_extern_variadic_function() {
        let (hir, diags) = lower_top_expr("fn extern foo(...)");

        assert_eq!(diags, 0);
        assert_eq!(
            hir,
            TopLevelExpression::ExternFunction(Box::new(
                DefineExternFunction {
                    documentation: String::new(),
                    public: false,
                    name: Identifier {
                        name: "foo".to_string(),
                        location: cols(11, 13)
                    },
                    arguments: Vec::new(),
                    variadic: true,
                    return_type: None,
                    method_id: None,
                    location: cols(1, 18),
                }
            )),
        );
    }

    #[test]
    fn test_lower_type() {
        let hir = lower_top_expr("type A[B: C] { let @a: B }").0;

        assert_eq!(
            hir,
            TopLevelExpression::Type(Box::new(DefineType {
                documentation: String::new(),
                public: false,
                semantics: TypeSemantics::Default,
                kind: TypeKind::Regular,
                type_id: None,
                name: Constant { name: "A".to_string(), location: cols(6, 6) },
                type_parameters: vec![TypeParameter {
                    type_parameter_id: None,
                    name: Constant {
                        name: "B".to_string(),
                        location: cols(8, 8)
                    },
                    requirements: vec![TypeName {
                        self_type: false,
                        source: None,
                        resolved_type: types::TypeRef::Unknown,
                        name: Constant {
                            name: "C".to_string(),
                            location: cols(11, 11)
                        },
                        arguments: Vec::new(),
                        location: cols(11, 11)
                    }],
                    mutable: false,
                    copy: false,
                    location: cols(8, 11)
                }],
                body: vec![TypeExpression::Field(Box::new(DefineField {
                    documentation: String::new(),
                    public: false,
                    mutable: false,
                    field_id: None,
                    name: Identifier {
                        name: "a".to_string(),
                        location: cols(20, 21)
                    },
                    value_type: Type::Named(Box::new(TypeName {
                        self_type: false,
                        source: None,
                        resolved_type: types::TypeRef::Unknown,
                        name: Constant {
                            name: "B".to_string(),
                            location: cols(24, 24)
                        },
                        arguments: Vec::new(),
                        location: cols(24, 24)
                    })),
                    location: cols(16, 24),
                }))],
                location: cols(1, 26)
            })),
        );
    }

    #[test]
    fn test_lower_extern_type() {
        let hir = lower_top_expr("type extern A { let @a: B }").0;

        assert_eq!(
            hir,
            TopLevelExpression::ExternType(Box::new(DefineExternType {
                documentation: String::new(),
                public: false,
                type_id: None,
                name: Constant {
                    name: "A".to_string(),
                    location: cols(13, 13)
                },
                fields: vec![DefineField {
                    documentation: String::new(),
                    public: false,
                    mutable: false,
                    field_id: None,
                    name: Identifier {
                        name: "a".to_string(),
                        location: cols(21, 22)
                    },
                    value_type: Type::Named(Box::new(TypeName {
                        self_type: false,
                        source: None,
                        resolved_type: types::TypeRef::Unknown,
                        name: Constant {
                            name: "B".to_string(),
                            location: cols(25, 25)
                        },
                        arguments: Vec::new(),
                        location: cols(25, 25)
                    })),
                    location: cols(17, 25),
                }],
                location: cols(1, 27)
            })),
        );
    }

    #[test]
    fn test_lower_copy_type() {
        let hir = lower_top_expr("type copy A { let @a: B }").0;

        assert_eq!(
            hir,
            TopLevelExpression::Type(Box::new(DefineType {
                documentation: String::new(),
                public: false,
                semantics: TypeSemantics::Copy,
                type_id: None,
                kind: TypeKind::Regular,
                name: Constant {
                    name: "A".to_string(),
                    location: cols(11, 11)
                },
                body: vec![TypeExpression::Field(Box::new(DefineField {
                    documentation: String::new(),
                    public: false,
                    mutable: false,
                    field_id: None,
                    name: Identifier {
                        name: "a".to_string(),
                        location: cols(19, 20)
                    },
                    value_type: Type::Named(Box::new(TypeName {
                        self_type: false,
                        source: None,
                        resolved_type: types::TypeRef::Unknown,
                        name: Constant {
                            name: "B".to_string(),
                            location: cols(23, 23)
                        },
                        arguments: Vec::new(),
                        location: cols(23, 23)
                    })),
                    location: cols(15, 23),
                }))],
                type_parameters: Vec::new(),
                location: cols(1, 25)
            })),
        );
    }

    #[test]
    fn test_lower_public_type() {
        let hir = lower_top_expr("type pub A {}").0;

        assert_eq!(
            hir,
            TopLevelExpression::Type(Box::new(DefineType {
                documentation: String::new(),
                public: true,
                semantics: TypeSemantics::Default,
                kind: TypeKind::Regular,
                type_id: None,
                name: Constant {
                    name: "A".to_string(),
                    location: cols(10, 10)
                },
                type_parameters: Vec::new(),
                body: Vec::new(),
                location: cols(1, 13)
            })),
        );
    }

    #[test]
    fn test_lower_type_with_public_field() {
        let hir = lower_top_expr("type A { let pub @a: A }").0;

        assert_eq!(
            hir,
            TopLevelExpression::Type(Box::new(DefineType {
                documentation: String::new(),
                public: false,
                semantics: TypeSemantics::Default,
                kind: TypeKind::Regular,
                type_id: None,
                name: Constant { name: "A".to_string(), location: cols(6, 6) },
                type_parameters: Vec::new(),
                body: vec![TypeExpression::Field(Box::new(DefineField {
                    documentation: String::new(),
                    public: true,
                    mutable: false,
                    field_id: None,
                    name: Identifier {
                        name: "a".to_string(),
                        location: cols(18, 19)
                    },
                    value_type: Type::Named(Box::new(TypeName {
                        self_type: false,
                        source: None,
                        resolved_type: types::TypeRef::Unknown,
                        name: Constant {
                            name: "A".to_string(),
                            location: cols(22, 22)
                        },
                        arguments: Vec::new(),
                        location: cols(22, 22)
                    })),
                    location: cols(10, 22)
                }))],
                location: cols(1, 24)
            })),
        );
    }

    #[test]
    fn test_lower_type_with_mutable_field() {
        let hir = lower_top_expr("type A { let mut @a: A }").0;

        assert_eq!(
            hir,
            TopLevelExpression::Type(Box::new(DefineType {
                documentation: String::new(),
                public: false,
                semantics: TypeSemantics::Default,
                kind: TypeKind::Regular,
                type_id: None,
                name: Constant { name: "A".to_string(), location: cols(6, 6) },
                type_parameters: Vec::new(),
                body: vec![TypeExpression::Field(Box::new(DefineField {
                    documentation: String::new(),
                    public: false,
                    mutable: true,
                    field_id: None,
                    name: Identifier {
                        name: "a".to_string(),
                        location: cols(18, 19)
                    },
                    value_type: Type::Named(Box::new(TypeName {
                        self_type: false,
                        source: None,
                        resolved_type: types::TypeRef::Unknown,
                        name: Constant {
                            name: "A".to_string(),
                            location: cols(22, 22)
                        },
                        arguments: Vec::new(),
                        location: cols(22, 22)
                    })),
                    location: cols(10, 22)
                }))],
                location: cols(1, 24)
            })),
        );
    }

    #[test]
    fn test_lower_builtin_type() {
        let hir = lower_top_expr("type builtin A[B: C] { let @a: B }").0;

        assert_eq!(
            hir,
            TopLevelExpression::Type(Box::new(DefineType {
                documentation: String::new(),
                public: false,
                semantics: TypeSemantics::Default,
                type_id: None,
                kind: TypeKind::Builtin,
                name: Constant {
                    name: "A".to_string(),
                    location: cols(14, 14)
                },
                type_parameters: vec![TypeParameter {
                    type_parameter_id: None,
                    name: Constant {
                        name: "B".to_string(),
                        location: cols(16, 16)
                    },
                    requirements: vec![TypeName {
                        self_type: false,
                        source: None,
                        resolved_type: types::TypeRef::Unknown,
                        name: Constant {
                            name: "C".to_string(),
                            location: cols(19, 19)
                        },
                        arguments: Vec::new(),
                        location: cols(19, 19)
                    }],
                    mutable: false,
                    copy: false,
                    location: cols(16, 19)
                }],
                body: vec![TypeExpression::Field(Box::new(DefineField {
                    documentation: String::new(),
                    public: false,
                    mutable: false,
                    field_id: None,
                    name: Identifier {
                        name: "a".to_string(),
                        location: cols(28, 29)
                    },
                    value_type: Type::Named(Box::new(TypeName {
                        self_type: false,
                        source: None,
                        resolved_type: types::TypeRef::Unknown,
                        name: Constant {
                            name: "B".to_string(),
                            location: cols(32, 32)
                        },
                        arguments: Vec::new(),
                        location: cols(32, 32)
                    })),
                    location: cols(24, 32),
                }))],
                location: cols(1, 34)
            })),
        );
    }

    #[test]
    fn test_lower_async_type() {
        let hir = lower_top_expr("type async A {}").0;

        assert_eq!(
            hir,
            TopLevelExpression::Type(Box::new(DefineType {
                documentation: String::new(),
                public: false,
                semantics: TypeSemantics::Default,
                type_id: None,
                kind: TypeKind::Async,
                name: Constant {
                    name: "A".to_string(),
                    location: cols(12, 12)
                },
                type_parameters: Vec::new(),
                body: Vec::new(),
                location: cols(1, 15)
            })),
        );
    }

    #[test]
    fn test_lower_type_with_static_method() {
        let hir =
            lower_top_expr("type A { fn static a[A](b: B) -> D { 10 } }").0;

        assert_eq!(
            hir,
            TopLevelExpression::Type(Box::new(DefineType {
                documentation: String::new(),
                public: false,
                semantics: TypeSemantics::Default,
                type_id: None,
                kind: TypeKind::Regular,
                name: Constant { name: "A".to_string(), location: cols(6, 6) },
                type_parameters: Vec::new(),
                body: vec![TypeExpression::StaticMethod(Box::new(
                    DefineStaticMethod {
                        inline: false,
                        documentation: String::new(),
                        public: false,
                        name: Identifier {
                            name: "a".to_string(),
                            location: cols(20, 20)
                        },
                        type_parameters: vec![TypeParameter {
                            type_parameter_id: None,
                            name: Constant {
                                name: "A".to_string(),
                                location: cols(22, 22)
                            },
                            requirements: Vec::new(),
                            mutable: false,
                            copy: false,
                            location: cols(22, 22)
                        }],
                        arguments: vec![MethodArgument {
                            name: Identifier {
                                name: "b".to_string(),
                                location: cols(25, 25)
                            },
                            value_type: Type::Named(Box::new(TypeName {
                                self_type: false,
                                source: None,
                                resolved_type: types::TypeRef::Unknown,
                                name: Constant {
                                    name: "B".to_string(),
                                    location: cols(28, 28)
                                },
                                arguments: Vec::new(),
                                location: cols(28, 28)
                            })),
                            location: cols(25, 28)
                        }],
                        return_type: Some(Type::Named(Box::new(TypeName {
                            self_type: false,
                            source: None,
                            resolved_type: types::TypeRef::Unknown,
                            name: Constant {
                                name: "D".to_string(),
                                location: cols(34, 34)
                            },
                            arguments: Vec::new(),
                            location: cols(34, 34)
                        }))),
                        body: vec![Expression::Int(Box::new(IntLiteral {
                            value: 10,
                            resolved_type: types::TypeRef::Unknown,
                            location: cols(38, 39)
                        }))],
                        method_id: None,
                        location: cols(10, 41),
                    }
                ))],
                location: cols(1, 43)
            })),
        );
    }

    #[test]
    fn test_lower_type_with_async_method() {
        let hir =
            lower_top_expr("type A { fn async a[A](b: B) -> D { 10 } }").0;

        assert_eq!(
            hir,
            TopLevelExpression::Type(Box::new(DefineType {
                documentation: String::new(),
                public: false,
                semantics: TypeSemantics::Default,
                type_id: None,
                kind: TypeKind::Regular,
                name: Constant { name: "A".to_string(), location: cols(6, 6) },
                type_parameters: Vec::new(),
                body: vec![TypeExpression::AsyncMethod(Box::new(
                    DefineAsyncMethod {
                        documentation: String::new(),
                        mutable: false,
                        public: false,
                        name: Identifier {
                            name: "a".to_string(),
                            location: cols(19, 19)
                        },
                        type_parameters: vec![TypeParameter {
                            type_parameter_id: None,
                            name: Constant {
                                name: "A".to_string(),
                                location: cols(21, 21)
                            },
                            requirements: Vec::new(),
                            mutable: false,
                            copy: false,
                            location: cols(21, 21)
                        }],
                        arguments: vec![MethodArgument {
                            name: Identifier {
                                name: "b".to_string(),
                                location: cols(24, 24)
                            },
                            value_type: Type::Named(Box::new(TypeName {
                                self_type: false,
                                source: None,
                                resolved_type: types::TypeRef::Unknown,
                                name: Constant {
                                    name: "B".to_string(),
                                    location: cols(27, 27)
                                },
                                arguments: Vec::new(),
                                location: cols(27, 27)
                            })),
                            location: cols(24, 27)
                        }],
                        return_type: Some(Type::Named(Box::new(TypeName {
                            self_type: false,
                            source: None,
                            resolved_type: types::TypeRef::Unknown,
                            name: Constant {
                                name: "D".to_string(),
                                location: cols(33, 33)
                            },
                            arguments: Vec::new(),
                            location: cols(33, 33)
                        }))),
                        body: vec![Expression::Int(Box::new(IntLiteral {
                            value: 10,
                            resolved_type: types::TypeRef::Unknown,
                            location: cols(37, 38)
                        }))],
                        method_id: None,
                        location: cols(10, 40),
                    }
                ))],
                location: cols(1, 42)
            })),
        );
    }

    #[test]
    fn test_lower_type_with_instance_method() {
        let hir = lower_top_expr("type A { fn a[A](b: B) -> D { 10 } }").0;

        assert_eq!(
            hir,
            TopLevelExpression::Type(Box::new(DefineType {
                documentation: String::new(),
                public: false,
                semantics: TypeSemantics::Default,
                type_id: None,
                kind: TypeKind::Regular,
                name: Constant { name: "A".to_string(), location: cols(6, 6) },
                type_parameters: Vec::new(),
                body: vec![TypeExpression::InstanceMethod(Box::new(
                    DefineInstanceMethod {
                        inline: false,
                        documentation: String::new(),
                        public: false,
                        kind: MethodKind::Regular,
                        name: Identifier {
                            name: "a".to_string(),
                            location: cols(13, 13)
                        },
                        type_parameters: vec![TypeParameter {
                            type_parameter_id: None,
                            name: Constant {
                                name: "A".to_string(),
                                location: cols(15, 15)
                            },
                            requirements: Vec::new(),
                            mutable: false,
                            copy: false,
                            location: cols(15, 15)
                        }],
                        arguments: vec![MethodArgument {
                            name: Identifier {
                                name: "b".to_string(),
                                location: cols(18, 18)
                            },
                            value_type: Type::Named(Box::new(TypeName {
                                self_type: false,
                                source: None,
                                resolved_type: types::TypeRef::Unknown,
                                name: Constant {
                                    name: "B".to_string(),
                                    location: cols(21, 21)
                                },
                                arguments: Vec::new(),
                                location: cols(21, 21)
                            })),
                            location: cols(18, 21)
                        }],
                        return_type: Some(Type::Named(Box::new(TypeName {
                            self_type: false,
                            source: None,
                            resolved_type: types::TypeRef::Unknown,
                            name: Constant {
                                name: "D".to_string(),
                                location: cols(27, 27)
                            },
                            arguments: Vec::new(),
                            location: cols(27, 27)
                        }))),
                        body: vec![Expression::Int(Box::new(IntLiteral {
                            value: 10,
                            resolved_type: types::TypeRef::Unknown,
                            location: cols(31, 32)
                        }))],
                        method_id: None,
                        location: cols(10, 34)
                    }
                ))],
                location: cols(1, 36)
            })),
        );
    }

    #[test]
    fn test_lower_type_with_inline_method() {
        let hir = lower_top_expr("type A { fn inline foo {} }").0;

        assert_eq!(
            hir,
            TopLevelExpression::Type(Box::new(DefineType {
                documentation: String::new(),
                public: false,
                semantics: TypeSemantics::Default,
                type_id: None,
                kind: TypeKind::Regular,
                name: Constant { name: "A".to_string(), location: cols(6, 6) },
                type_parameters: Vec::new(),
                body: vec![TypeExpression::InstanceMethod(Box::new(
                    DefineInstanceMethod {
                        inline: true,
                        documentation: String::new(),
                        public: false,
                        kind: MethodKind::Regular,
                        name: Identifier {
                            name: "foo".to_string(),
                            location: cols(20, 22)
                        },
                        type_parameters: Vec::new(),
                        arguments: Vec::new(),
                        return_type: None,
                        body: Vec::new(),
                        method_id: None,
                        location: cols(10, 25)
                    }
                ))],
                location: cols(1, 27)
            })),
        );
    }

    #[test]
    fn test_lower_static_operator_method() {
        let diags = lower_top_expr("type A { fn static + {} }").1;

        assert_eq!(diags, 1);
    }

    #[test]
    fn test_lower_module_operator_method() {
        let diags = lower_top_expr("type A { fn static + {} }").1;

        assert_eq!(diags, 1);
    }

    #[test]
    fn test_lower_trait() {
        let hir = lower_top_expr("trait A[T]: B {}").0;

        assert_eq!(
            hir,
            TopLevelExpression::Trait(Box::new(DefineTrait {
                documentation: String::new(),
                public: false,
                trait_id: None,
                name: Constant { name: "A".to_string(), location: cols(7, 7) },
                type_parameters: vec![TypeParameter {
                    type_parameter_id: None,
                    name: Constant {
                        name: "T".to_string(),
                        location: cols(9, 9)
                    },
                    requirements: Vec::new(),
                    mutable: false,
                    copy: false,
                    location: cols(9, 9)
                }],
                requirements: vec![TypeName {
                    self_type: false,
                    source: None,
                    resolved_type: types::TypeRef::Unknown,
                    name: Constant {
                        name: "B".to_string(),
                        location: cols(13, 13)
                    },
                    arguments: Vec::new(),
                    location: cols(13, 13)
                }],
                body: Vec::new(),
                location: cols(1, 16)
            })),
        );
    }

    #[test]
    fn test_lower_public_trait() {
        let hir = lower_top_expr("trait pub A {}").0;

        assert_eq!(
            hir,
            TopLevelExpression::Trait(Box::new(DefineTrait {
                documentation: String::new(),
                public: true,
                trait_id: None,
                name: Constant {
                    name: "A".to_string(),
                    location: cols(11, 11)
                },
                type_parameters: Vec::new(),
                requirements: Vec::new(),
                body: Vec::new(),
                location: cols(1, 14)
            })),
        );
    }

    #[test]
    fn test_lower_trait_with_required_method() {
        let hir = lower_top_expr("trait A { fn a[A](b: B) -> D }").0;

        assert_eq!(
            hir,
            TopLevelExpression::Trait(Box::new(DefineTrait {
                documentation: String::new(),
                public: false,
                trait_id: None,
                name: Constant { name: "A".to_string(), location: cols(7, 7) },
                requirements: Vec::new(),
                type_parameters: Vec::new(),
                body: vec![TraitExpression::RequiredMethod(Box::new(
                    DefineRequiredMethod {
                        documentation: String::new(),
                        public: false,
                        kind: MethodKind::Regular,
                        name: Identifier {
                            name: "a".to_string(),
                            location: cols(14, 14)
                        },
                        type_parameters: vec![TypeParameter {
                            type_parameter_id: None,
                            name: Constant {
                                name: "A".to_string(),
                                location: cols(16, 16)
                            },
                            requirements: Vec::new(),
                            mutable: false,
                            copy: false,
                            location: cols(16, 16)
                        }],
                        arguments: vec![MethodArgument {
                            name: Identifier {
                                name: "b".to_string(),
                                location: cols(19, 19)
                            },
                            value_type: Type::Named(Box::new(TypeName {
                                self_type: false,
                                source: None,
                                resolved_type: types::TypeRef::Unknown,
                                name: Constant {
                                    name: "B".to_string(),
                                    location: cols(22, 22)
                                },
                                arguments: Vec::new(),
                                location: cols(22, 22)
                            })),
                            location: cols(19, 22)
                        }],
                        return_type: Some(Type::Named(Box::new(TypeName {
                            self_type: false,
                            source: None,
                            resolved_type: types::TypeRef::Unknown,
                            name: Constant {
                                name: "D".to_string(),
                                location: cols(28, 28)
                            },
                            arguments: Vec::new(),
                            location: cols(28, 28)
                        }))),
                        method_id: None,
                        location: cols(11, 28)
                    }
                ))],
                location: cols(1, 30)
            }))
        );
    }

    #[test]
    fn test_lower_trait_with_moving_required_method() {
        let hir = lower_top_expr("trait A { fn move a }").0;

        assert_eq!(
            hir,
            TopLevelExpression::Trait(Box::new(DefineTrait {
                documentation: String::new(),
                public: false,
                trait_id: None,
                name: Constant { name: "A".to_string(), location: cols(7, 7) },
                requirements: Vec::new(),
                type_parameters: Vec::new(),
                body: vec![TraitExpression::RequiredMethod(Box::new(
                    DefineRequiredMethod {
                        documentation: String::new(),
                        public: false,
                        kind: MethodKind::Moving,
                        name: Identifier {
                            name: "a".to_string(),
                            location: cols(19, 19)
                        },
                        type_parameters: Vec::new(),
                        arguments: Vec::new(),
                        return_type: None,
                        method_id: None,
                        location: cols(11, 19)
                    }
                ))],
                location: cols(1, 21)
            }))
        );
    }

    #[test]
    fn test_lower_trait_with_moving_default_method() {
        let hir = lower_top_expr("trait A { fn move a {} }").0;

        assert_eq!(
            hir,
            TopLevelExpression::Trait(Box::new(DefineTrait {
                documentation: String::new(),
                public: false,
                trait_id: None,
                name: Constant { name: "A".to_string(), location: cols(7, 7) },
                requirements: Vec::new(),
                type_parameters: Vec::new(),
                body: vec![TraitExpression::InstanceMethod(Box::new(
                    DefineInstanceMethod {
                        inline: false,
                        documentation: String::new(),
                        public: false,
                        kind: MethodKind::Moving,
                        name: Identifier {
                            name: "a".to_string(),
                            location: cols(19, 19)
                        },
                        type_parameters: Vec::new(),
                        arguments: Vec::new(),
                        return_type: None,
                        body: Vec::new(),
                        method_id: None,
                        location: cols(11, 22)
                    }
                ))],
                location: cols(1, 24)
            }))
        );
    }

    #[test]
    fn test_lower_trait_with_default_method() {
        let hir = lower_top_expr("trait A { fn a[A](b: B) -> D { 10 } }").0;

        assert_eq!(
            hir,
            TopLevelExpression::Trait(Box::new(DefineTrait {
                documentation: String::new(),
                public: false,
                trait_id: None,
                name: Constant { name: "A".to_string(), location: cols(7, 7) },
                requirements: Vec::new(),
                type_parameters: Vec::new(),
                body: vec![TraitExpression::InstanceMethod(Box::new(
                    DefineInstanceMethod {
                        inline: false,
                        documentation: String::new(),
                        public: false,
                        kind: MethodKind::Regular,
                        name: Identifier {
                            name: "a".to_string(),
                            location: cols(14, 14)
                        },
                        type_parameters: vec![TypeParameter {
                            type_parameter_id: None,
                            name: Constant {
                                name: "A".to_string(),
                                location: cols(16, 16)
                            },
                            requirements: Vec::new(),
                            mutable: false,
                            copy: false,
                            location: cols(16, 16)
                        }],
                        arguments: vec![MethodArgument {
                            name: Identifier {
                                name: "b".to_string(),
                                location: cols(19, 19)
                            },
                            value_type: Type::Named(Box::new(TypeName {
                                self_type: false,
                                source: None,
                                resolved_type: types::TypeRef::Unknown,
                                name: Constant {
                                    name: "B".to_string(),
                                    location: cols(22, 22)
                                },
                                arguments: Vec::new(),
                                location: cols(22, 22)
                            })),
                            location: cols(19, 22)
                        }],
                        return_type: Some(Type::Named(Box::new(TypeName {
                            self_type: false,
                            source: None,
                            resolved_type: types::TypeRef::Unknown,
                            name: Constant {
                                name: "D".to_string(),
                                location: cols(28, 28)
                            },
                            arguments: Vec::new(),
                            location: cols(28, 28)
                        }))),
                        body: vec![Expression::Int(Box::new(IntLiteral {
                            value: 10,
                            resolved_type: types::TypeRef::Unknown,
                            location: cols(32, 33)
                        }))],
                        method_id: None,
                        location: cols(11, 35)
                    }
                ))],
                location: cols(1, 37)
            }))
        );
    }

    #[test]
    fn test_lower_trait_with_inline_method() {
        let hir = lower_top_expr("trait A { fn inline foo {} }").0;

        assert_eq!(
            hir,
            TopLevelExpression::Trait(Box::new(DefineTrait {
                documentation: String::new(),
                public: false,
                trait_id: None,
                name: Constant { name: "A".to_string(), location: cols(7, 7) },
                requirements: Vec::new(),
                type_parameters: Vec::new(),
                body: vec![TraitExpression::InstanceMethod(Box::new(
                    DefineInstanceMethod {
                        inline: true,
                        documentation: String::new(),
                        public: false,
                        kind: MethodKind::Regular,
                        name: Identifier {
                            name: "foo".to_string(),
                            location: cols(21, 23)
                        },
                        type_parameters: Vec::new(),
                        arguments: Vec::new(),
                        return_type: None,
                        body: Vec::new(),
                        method_id: None,
                        location: cols(11, 26)
                    }
                ))],
                location: cols(1, 28)
            }))
        );
    }

    #[test]
    fn test_lower_reopen_empty_type() {
        assert_eq!(
            lower_top_expr("impl A {}").0,
            TopLevelExpression::Reopen(Box::new(ReopenType {
                type_id: None,
                type_name: Constant {
                    name: "A".to_string(),
                    location: cols(6, 6)
                },
                bounds: Vec::new(),
                body: Vec::new(),
                location: cols(1, 9)
            }))
        );

        assert_eq!(
            lower_top_expr("impl A if T: mut {}").0,
            TopLevelExpression::Reopen(Box::new(ReopenType {
                type_id: None,
                type_name: Constant {
                    name: "A".to_string(),
                    location: cols(6, 6)
                },
                bounds: vec![TypeBound {
                    name: Constant {
                        name: "T".to_string(),
                        location: cols(11, 11)
                    },
                    requirements: Vec::new(),
                    mutable: true,
                    copy: false,
                    location: cols(11, 16),
                }],
                body: Vec::new(),
                location: cols(1, 16)
            }))
        );
    }

    #[test]
    fn test_lower_reopen_type_with_instance_method() {
        let hir = lower_top_expr("impl A { fn foo {} }").0;

        assert_eq!(
            hir,
            TopLevelExpression::Reopen(Box::new(ReopenType {
                type_id: None,
                type_name: Constant {
                    name: "A".to_string(),
                    location: cols(6, 6)
                },
                body: vec![ReopenTypeExpression::InstanceMethod(Box::new(
                    DefineInstanceMethod {
                        inline: false,
                        documentation: String::new(),
                        public: false,
                        kind: MethodKind::Regular,
                        name: Identifier {
                            name: "foo".to_string(),
                            location: cols(13, 15)
                        },
                        type_parameters: Vec::new(),
                        arguments: Vec::new(),
                        return_type: None,
                        body: Vec::new(),
                        method_id: None,
                        location: cols(10, 18)
                    }
                ))],
                bounds: Vec::new(),
                location: cols(1, 20)
            }))
        );
    }

    #[test]
    fn test_lower_reopen_type_with_static_method() {
        let hir = lower_top_expr("impl A { fn static foo {} }").0;

        assert_eq!(
            hir,
            TopLevelExpression::Reopen(Box::new(ReopenType {
                type_id: None,
                type_name: Constant {
                    name: "A".to_string(),
                    location: cols(6, 6)
                },
                body: vec![ReopenTypeExpression::StaticMethod(Box::new(
                    DefineStaticMethod {
                        inline: false,
                        documentation: String::new(),
                        public: false,
                        name: Identifier {
                            name: "foo".to_string(),
                            location: cols(20, 22)
                        },
                        type_parameters: Vec::new(),
                        arguments: Vec::new(),
                        return_type: None,
                        body: Vec::new(),
                        method_id: None,
                        location: cols(10, 25),
                    }
                ))],
                bounds: Vec::new(),
                location: cols(1, 27)
            }))
        );
    }

    #[test]
    fn test_lower_reopen_type_with_async_method() {
        let hir = lower_top_expr("impl A { fn async foo {} }").0;

        assert_eq!(
            hir,
            TopLevelExpression::Reopen(Box::new(ReopenType {
                type_id: None,
                type_name: Constant {
                    name: "A".to_string(),
                    location: cols(6, 6)
                },
                body: vec![ReopenTypeExpression::AsyncMethod(Box::new(
                    DefineAsyncMethod {
                        documentation: String::new(),
                        mutable: false,
                        public: false,
                        name: Identifier {
                            name: "foo".to_string(),
                            location: cols(19, 21)
                        },
                        type_parameters: Vec::new(),
                        arguments: Vec::new(),
                        return_type: None,
                        body: Vec::new(),
                        method_id: None,
                        location: cols(10, 24)
                    }
                ))],
                bounds: Vec::new(),
                location: cols(1, 26)
            }))
        );
    }

    #[test]
    fn test_lower_reopen_type_with_async_mutable_method() {
        let hir = lower_top_expr("impl A { fn async mut foo {} }").0;

        assert_eq!(
            hir,
            TopLevelExpression::Reopen(Box::new(ReopenType {
                type_id: None,
                type_name: Constant {
                    name: "A".to_string(),
                    location: cols(6, 6)
                },
                body: vec![ReopenTypeExpression::AsyncMethod(Box::new(
                    DefineAsyncMethod {
                        documentation: String::new(),
                        mutable: true,
                        public: false,
                        name: Identifier {
                            name: "foo".to_string(),
                            location: cols(23, 25)
                        },
                        type_parameters: Vec::new(),
                        arguments: Vec::new(),
                        return_type: None,
                        body: Vec::new(),
                        method_id: None,
                        location: cols(10, 28)
                    }
                ))],
                bounds: Vec::new(),
                location: cols(1, 30)
            }))
        );
    }

    #[test]
    fn test_lower_empty_trait_implementation() {
        let hir = lower_top_expr("impl A for B {}").0;

        assert_eq!(
            hir,
            TopLevelExpression::Implement(Box::new(ImplementTrait {
                trait_name: TypeName {
                    self_type: false,
                    source: None,
                    resolved_type: types::TypeRef::Unknown,
                    name: Constant {
                        name: "A".to_string(),
                        location: cols(6, 6)
                    },
                    arguments: Vec::new(),
                    location: cols(6, 6)
                },
                type_name: Constant {
                    name: "B".to_string(),
                    location: cols(12, 12)
                },
                bounds: Vec::new(),
                body: Vec::new(),
                location: cols(1, 15),
                trait_instance: None,
                type_instance: None,
            }))
        );
    }

    #[test]
    fn test_lower_trait_implementation_with_type_argument() {
        let hir = lower_top_expr("impl A[T] for B {}").0;

        assert_eq!(
            hir,
            TopLevelExpression::Implement(Box::new(ImplementTrait {
                trait_name: TypeName {
                    self_type: false,
                    source: None,
                    resolved_type: types::TypeRef::Unknown,
                    name: Constant {
                        name: "A".to_string(),
                        location: cols(6, 6)
                    },
                    arguments: vec![Type::Named(Box::new(TypeName {
                        self_type: false,
                        source: None,
                        resolved_type: types::TypeRef::Unknown,
                        name: Constant {
                            name: "T".to_string(),
                            location: cols(8, 8)
                        },
                        arguments: Vec::new(),
                        location: cols(8, 8)
                    }))],
                    location: cols(6, 9)
                },
                type_name: Constant {
                    name: "B".to_string(),
                    location: cols(15, 15)
                },
                bounds: Vec::new(),
                body: Vec::new(),
                location: cols(1, 18),
                trait_instance: None,
                type_instance: None,
            }))
        );
    }

    #[test]
    fn test_lower_trait_implementation_with_bounds() {
        let hir = lower_top_expr("impl A for B if T: X + mut {}").0;

        assert_eq!(
            hir,
            TopLevelExpression::Implement(Box::new(ImplementTrait {
                trait_name: TypeName {
                    self_type: false,
                    source: None,
                    resolved_type: types::TypeRef::Unknown,
                    name: Constant {
                        name: "A".to_string(),
                        location: cols(6, 6)
                    },
                    arguments: Vec::new(),
                    location: cols(6, 6)
                },
                type_name: Constant {
                    name: "B".to_string(),
                    location: cols(12, 12)
                },
                bounds: vec![TypeBound {
                    name: Constant {
                        name: "T".to_string(),
                        location: cols(17, 17)
                    },
                    requirements: vec![TypeName {
                        self_type: false,
                        source: None,
                        resolved_type: types::TypeRef::Unknown,
                        name: Constant {
                            name: "X".to_string(),
                            location: cols(20, 20)
                        },
                        arguments: Vec::new(),
                        location: cols(20, 20)
                    },],
                    mutable: true,
                    copy: false,
                    location: cols(17, 26)
                }],
                body: Vec::new(),
                location: cols(1, 29),
                trait_instance: None,
                type_instance: None,
            }))
        );
    }

    #[test]
    fn test_lower_trait_implementation_with_instance_method() {
        let hir = lower_top_expr("impl A for B { fn foo {} }").0;

        assert_eq!(
            hir,
            TopLevelExpression::Implement(Box::new(ImplementTrait {
                trait_name: TypeName {
                    self_type: false,
                    source: None,
                    resolved_type: types::TypeRef::Unknown,
                    name: Constant {
                        name: "A".to_string(),
                        location: cols(6, 6)
                    },
                    arguments: Vec::new(),
                    location: cols(6, 6)
                },
                type_name: Constant {
                    name: "B".to_string(),
                    location: cols(12, 12)
                },
                bounds: Vec::new(),
                body: vec![DefineInstanceMethod {
                    inline: false,
                    documentation: String::new(),
                    public: false,
                    kind: MethodKind::Regular,
                    name: Identifier {
                        name: "foo".to_string(),
                        location: cols(19, 21)
                    },
                    type_parameters: Vec::new(),
                    arguments: Vec::new(),
                    return_type: None,
                    body: Vec::new(),
                    method_id: None,
                    location: cols(16, 24)
                }],
                location: cols(1, 26),
                trait_instance: None,
                type_instance: None,
            }))
        );
    }

    #[test]
    fn test_lower_trait_implementation_with_moving_method() {
        let hir = lower_top_expr("impl A for B { fn move foo {} }").0;

        assert_eq!(
            hir,
            TopLevelExpression::Implement(Box::new(ImplementTrait {
                trait_name: TypeName {
                    self_type: false,
                    source: None,
                    resolved_type: types::TypeRef::Unknown,
                    name: Constant {
                        name: "A".to_string(),
                        location: cols(6, 6)
                    },
                    arguments: Vec::new(),
                    location: cols(6, 6)
                },
                type_name: Constant {
                    name: "B".to_string(),
                    location: cols(12, 12)
                },
                bounds: Vec::new(),
                body: vec![DefineInstanceMethod {
                    inline: false,
                    documentation: String::new(),
                    public: false,
                    kind: MethodKind::Moving,
                    name: Identifier {
                        name: "foo".to_string(),
                        location: cols(24, 26)
                    },
                    type_parameters: Vec::new(),
                    arguments: Vec::new(),
                    return_type: None,
                    body: Vec::new(),
                    method_id: None,
                    location: cols(16, 29)
                }],
                location: cols(1, 31),
                trait_instance: None,
                type_instance: None,
            }))
        );
    }

    #[test]
    fn test_lower_import() {
        let hir = lower_top_expr("import a").0;

        assert_eq!(
            hir,
            TopLevelExpression::Import(Box::new(Import {
                source: vec![Identifier {
                    name: "a".to_string(),
                    location: cols(8, 8)
                }],
                symbols: Vec::new(),
                location: cols(1, 8)
            }))
        );
    }

    #[test]
    fn test_lower_extern_import() {
        let hir = lower_top_expr("import extern 'a'").0;

        assert_eq!(
            hir,
            TopLevelExpression::ExternImport(Box::new(ExternImport {
                source: "a".to_string(),
                location: cols(1, 17)
            }))
        );
    }

    #[test]
    fn test_lower_import_symbol() {
        let hir = lower_top_expr("import a (b)").0;

        assert_eq!(
            hir,
            TopLevelExpression::Import(Box::new(Import {
                source: vec![Identifier {
                    name: "a".to_string(),
                    location: cols(8, 8)
                }],
                symbols: vec![ImportSymbol {
                    name: Identifier {
                        name: "b".to_string(),
                        location: cols(11, 11)
                    },
                    import_as: Identifier {
                        name: "b".to_string(),
                        location: cols(11, 11)
                    },
                    location: cols(11, 11)
                }],
                location: cols(1, 12)
            }))
        );
    }

    #[test]
    fn test_lower_import_symbol_with_alias() {
        let hir = lower_top_expr("import a (b as c)").0;

        assert_eq!(
            hir,
            TopLevelExpression::Import(Box::new(Import {
                source: vec![Identifier {
                    name: "a".to_string(),
                    location: cols(8, 8)
                }],
                symbols: vec![ImportSymbol {
                    name: Identifier {
                        name: "b".to_string(),
                        location: cols(11, 11)
                    },
                    import_as: Identifier {
                        name: "c".to_string(),
                        location: cols(16, 16)
                    },
                    location: cols(11, 16)
                }],
                location: cols(1, 17)
            }))
        );
    }

    #[test]
    fn test_lower_import_self() {
        let hir = lower_top_expr("import a (self)").0;

        assert_eq!(
            hir,
            TopLevelExpression::Import(Box::new(Import {
                source: vec![Identifier {
                    name: "a".to_string(),
                    location: cols(8, 8)
                }],
                symbols: vec![ImportSymbol {
                    name: Identifier {
                        name: "self".to_string(),
                        location: cols(11, 14)
                    },
                    import_as: Identifier {
                        name: "self".to_string(),
                        location: cols(11, 14)
                    },
                    location: cols(11, 14)
                }],
                location: cols(1, 15)
            }))
        );
    }

    #[test]
    fn test_lower_int() {
        let hir = lower_expr("fn a { 1_0 }").0;

        assert_eq!(
            hir,
            Expression::Int(Box::new(IntLiteral {
                value: 10,
                resolved_type: types::TypeRef::Unknown,
                location: cols(8, 10)
            }))
        );
    }

    #[test]
    fn test_lower_hex_int() {
        let hir = lower_expr("fn a { 0xff }").0;

        assert_eq!(
            hir,
            Expression::Int(Box::new(IntLiteral {
                value: 0xff,
                resolved_type: types::TypeRef::Unknown,
                location: cols(8, 11)
            }))
        );
    }

    #[test]
    fn test_lower_negative_hex_int() {
        let hir = lower_expr("fn a { -0x4a3f043013b2c4d1 }").0;

        assert_eq!(
            hir,
            Expression::Int(Box::new(IntLiteral {
                value: -0x4a3f043013b2c4d1,
                resolved_type: types::TypeRef::Unknown,
                location: cols(8, 26)
            }))
        );
    }

    #[test]
    fn test_lower_float() {
        let hir = lower_expr("fn a { 1_0.5 }").0;

        assert_eq!(
            hir,
            Expression::Float(Box::new(FloatLiteral {
                value: 10.5,
                resolved_type: types::TypeRef::Unknown,
                location: cols(8, 12)
            }))
        );
    }

    #[test]
    fn test_lower_single_string() {
        let hir = lower_expr("fn a { 'a' }").0;

        assert_eq!(
            hir,
            Expression::String(Box::new(StringLiteral {
                value: "a".to_string(),
                resolved_type: types::TypeRef::Unknown,
                location: cols(8, 10)
            }))
        );
    }

    #[test]
    fn test_lower_double_string() {
        let hir = lower_expr("fn a { \"a\" }").0;

        assert_eq!(
            hir,
            Expression::String(Box::new(StringLiteral {
                value: "a".to_string(),
                resolved_type: types::TypeRef::Unknown,
                location: cols(8, 10)
            }))
        );
    }

    #[test]
    fn test_lower_double_string_with_escape() {
        let hir = lower_expr("fn a { \"a\\u{AC}\" }").0;

        assert_eq!(
            hir,
            Expression::String(Box::new(StringLiteral {
                value: "a\u{AC}".to_string(),
                resolved_type: types::TypeRef::Unknown,
                location: cols(8, 16)
            }))
        );
    }

    #[test]
    fn test_lower_double_string_with_interpolation() {
        let hir = lower_expr("fn a { \"a${10}b\" }").0;

        assert_eq!(
            hir,
            Expression::Scope(Box::new(Scope {
                resolved_type: types::TypeRef::Unknown,
                body: vec![
                    Expression::define_variable(
                        STR_BUF_VAR,
                        Expression::call_static(
                            STRING_BUFFER_INTERNAL_NAME,
                            STRING_BUFFER_NEW,
                            Vec::new(),
                            cols(8, 16),
                        ),
                        cols(8, 16),
                    ),
                    Expression::call(
                        Expression::identifier_ref(STR_BUF_VAR, cols(8, 16)),
                        STRING_BUFFER_PUSH,
                        vec![Expression::string("a".to_string(), cols(9, 9))],
                        cols(9, 9)
                    ),
                    Expression::call(
                        Expression::identifier_ref(STR_BUF_VAR, cols(8, 16)),
                        STRING_BUFFER_PUSH,
                        vec![Expression::call(
                            Expression::Int(Box::new(IntLiteral {
                                value: 10,
                                resolved_type: types::TypeRef::Unknown,
                                location: cols(12, 13)
                            })),
                            TO_STRING_METHOD,
                            Vec::new(),
                            cols(12, 13)
                        )],
                        cols(12, 13)
                    ),
                    Expression::call(
                        Expression::identifier_ref(STR_BUF_VAR, cols(8, 16)),
                        STRING_BUFFER_PUSH,
                        vec![Expression::string("b".to_string(), cols(15, 15))],
                        cols(15, 15)
                    ),
                    Expression::call(
                        Expression::identifier_ref(STR_BUF_VAR, cols(8, 16)),
                        STRING_BUFFER_INTO_STRING,
                        Vec::new(),
                        cols(8, 16)
                    )
                ],
                location: cols(8, 16)
            }))
        );
    }

    #[test]
    fn test_lower_array() {
        let hir = lower_expr("fn a { [10] }").0;

        assert_eq!(
            hir,
            Expression::Scope(Box::new(Scope {
                resolved_type: types::TypeRef::Unknown,
                body: vec![
                    Expression::define_variable(
                        ARRAY_LIT_VAR,
                        Expression::call_static(
                            ARRAY_INTERNAL_NAME,
                            ARRAY_WITH_CAPACITY,
                            vec![Expression::Int(Box::new(IntLiteral {
                                value: 1,
                                resolved_type: types::TypeRef::Unknown,
                                location: cols(8, 11),
                            }))],
                            cols(8, 11)
                        ),
                        cols(8, 11)
                    ),
                    Expression::call(
                        Expression::identifier_ref(ARRAY_LIT_VAR, cols(8, 11)),
                        ARRAY_PUSH,
                        vec![Expression::Int(Box::new(IntLiteral {
                            value: 10,
                            resolved_type: types::TypeRef::Unknown,
                            location: cols(9, 10),
                        }))],
                        cols(9, 10)
                    ),
                    Expression::identifier_ref(ARRAY_LIT_VAR, cols(8, 11))
                ],
                location: cols(8, 11),
            }))
        );
    }

    #[test]
    fn test_lower_array_with_comments() {
        let hir = lower_expr_with_comments(
            "fn a {
              [
                # foo
              ]
            }",
        )
        .0;

        assert_eq!(
            hir,
            Expression::Scope(Box::new(Scope {
                resolved_type: types::TypeRef::Unknown,
                body: vec![
                    Expression::DefineVariable(Box::new(DefineVariable {
                        resolved_type: types::TypeRef::Unknown,
                        variable_id: None,
                        mutable: false,
                        name: Identifier {
                            name: "$array".to_string(),
                            location: loc(2, 4, 15, 15),
                        },
                        value_type: None,
                        value: Expression::Call(Box::new(Call {
                            kind: types::CallKind::Unknown,
                            receiver: Some(Expression::ConstantRef(Box::new(
                                ConstantRef {
                                    kind: types::ConstantKind::Unknown,
                                    source: None,
                                    name: "$Array".to_string(),
                                    resolved_type: types::TypeRef::Unknown,
                                    usage: Usage::Used,
                                    location: loc(2, 4, 15, 15),
                                }
                            ))),
                            name: Identifier {
                                name: "with_capacity".to_string(),
                                location: loc(2, 4, 15, 15),
                            },
                            parens: true,
                            in_mut: false,
                            usage: Usage::Used,
                            arguments: vec![Argument::Positional(Box::new(
                                PositionalArgument {
                                    value: Expression::Int(Box::new(
                                        IntLiteral {
                                            value: 0,
                                            resolved_type:
                                                types::TypeRef::Unknown,
                                            location: loc(2, 4, 15, 15),
                                        }
                                    )),
                                    expected_type: types::TypeRef::Unknown,
                                }
                            ))],
                            location: loc(2, 4, 15, 15),
                        },)),
                        location: loc(2, 4, 15, 15),
                    })),
                    Expression::IdentifierRef(Box::new(IdentifierRef {
                        name: "$array".to_string(),
                        kind: types::IdentifierKind::Unknown,
                        usage: Usage::Used,
                        location: loc(2, 4, 15, 15),
                    })),
                ],
                location: loc(2, 4, 15, 15),
            }))
        );
    }

    #[test]
    fn test_lower_tuple_with_comments() {
        let hir = lower_expr_with_comments(
            "fn a {
  (
    10,
    # foo
  )
}",
        )
        .0;

        assert_eq!(
            hir,
            Expression::Tuple(Box::new(TupleLiteral {
                type_id: None,
                resolved_type: types::TypeRef::Unknown,
                value_types: Vec::new(),
                values: vec![Expression::Int(Box::new(IntLiteral {
                    resolved_type: types::TypeRef::Unknown,
                    value: 10,
                    location: loc(3, 3, 5, 6)
                }))],
                location: loc(2, 5, 3, 3)
            }))
        );
    }

    #[test]
    fn test_lower_tuple() {
        let hir = lower_expr("fn a { (10,) }").0;

        assert_eq!(
            hir,
            Expression::Tuple(Box::new(TupleLiteral {
                type_id: None,
                resolved_type: types::TypeRef::Unknown,
                value_types: Vec::new(),
                values: vec![Expression::Int(Box::new(IntLiteral {
                    resolved_type: types::TypeRef::Unknown,
                    value: 10,
                    location: cols(9, 10)
                }))],
                location: cols(8, 12)
            }))
        );
    }

    #[test]
    fn test_lower_binary() {
        let hir = lower_expr("fn a { 1 + 2 }").0;

        assert_eq!(
            hir,
            Expression::Call(Box::new(Call {
                kind: types::CallKind::Unknown,
                receiver: Some(Expression::Int(Box::new(IntLiteral {
                    value: 1,
                    resolved_type: types::TypeRef::Unknown,
                    location: cols(8, 8)
                }))),
                parens: true,
                in_mut: false,
                usage: Usage::Used,
                arguments: vec![Argument::Positional(Box::new(
                    PositionalArgument {
                        value: Expression::Int(Box::new(IntLiteral {
                            value: 2,
                            resolved_type: types::TypeRef::Unknown,
                            location: cols(12, 12)
                        })),
                        expected_type: types::TypeRef::Unknown,
                    }
                ))],
                name: Identifier {
                    name: Operator::Add.method_name().to_string(),
                    location: cols(10, 10)
                },
                location: cols(8, 12)
            }))
        );
    }

    #[test]
    fn test_lower_field() {
        let hir = lower_expr("fn a { @a }").0;

        assert_eq!(
            hir,
            Expression::FieldRef(Box::new(FieldRef {
                info: None,
                name: "a".to_string(),
                in_mut: false,
                location: cols(8, 9)
            }))
        );
    }

    #[test]
    fn test_lower_constant() {
        let hir = lower_expr("fn a { A }").0;

        assert_eq!(
            hir,
            Expression::ConstantRef(Box::new(ConstantRef {
                kind: types::ConstantKind::Unknown,
                source: None,
                name: "A".to_string(),
                resolved_type: types::TypeRef::Unknown,
                usage: Usage::Used,
                location: cols(8, 8)
            }))
        );
    }

    #[test]
    fn test_lower_namespaced_constant() {
        let hir = lower_expr("fn a { a.B }").0;

        assert_eq!(
            hir,
            Expression::Call(Box::new(Call {
                kind: types::CallKind::Unknown,
                receiver: Some(Expression::IdentifierRef(Box::new(
                    IdentifierRef {
                        kind: types::IdentifierKind::Unknown,
                        name: "a".to_string(),
                        usage: Usage::Used,
                        location: cols(8, 8)
                    }
                ))),
                name: Identifier {
                    name: "B".to_string(),
                    location: cols(10, 10)
                },
                parens: false,
                in_mut: false,
                usage: Usage::Used,
                arguments: Vec::new(),
                location: cols(8, 10)
            }))
        );
    }

    #[test]
    fn test_lower_identifier() {
        let hir = lower_expr("fn a { a }").0;

        assert_eq!(
            hir,
            Expression::IdentifierRef(Box::new(IdentifierRef {
                kind: types::IdentifierKind::Unknown,
                name: "a".to_string(),
                usage: Usage::Used,
                location: cols(8, 8)
            }))
        );
    }

    #[test]
    fn test_lower_call_with_positional_argument() {
        let hir = lower_expr("fn a { a(10) }").0;

        assert_eq!(
            hir,
            Expression::Call(Box::new(Call {
                kind: types::CallKind::Unknown,
                receiver: None,
                name: Identifier {
                    name: "a".to_string(),
                    location: cols(8, 8)
                },
                parens: true,
                in_mut: false,
                usage: Usage::Used,
                arguments: vec![Argument::Positional(Box::new(
                    PositionalArgument {
                        value: Expression::Int(Box::new(IntLiteral {
                            value: 10,
                            resolved_type: types::TypeRef::Unknown,
                            location: cols(10, 11)
                        })),
                        expected_type: types::TypeRef::Unknown,
                    }
                ))],
                location: cols(8, 12)
            }))
        );
    }

    #[test]
    fn test_lower_call_with_named_argument() {
        let hir = lower_expr("fn a { a(b: 10) }").0;

        assert_eq!(
            hir,
            Expression::Call(Box::new(Call {
                kind: types::CallKind::Unknown,
                receiver: None,
                name: Identifier {
                    name: "a".to_string(),
                    location: cols(8, 8)
                },
                parens: true,
                in_mut: false,
                usage: Usage::Used,
                arguments: vec![Argument::Named(Box::new(NamedArgument {
                    index: 0,
                    name: Identifier {
                        name: "b".to_string(),
                        location: cols(10, 10)
                    },
                    value: Expression::Int(Box::new(IntLiteral {
                        value: 10,
                        resolved_type: types::TypeRef::Unknown,
                        location: cols(13, 14)
                    })),
                    location: cols(10, 14),
                    expected_type: types::TypeRef::Unknown,
                }))],
                location: cols(8, 15)
            }))
        );
    }

    #[test]
    fn test_call_with_receiver() {
        let hir = lower_expr("fn a { a.b }").0;

        assert_eq!(
            hir,
            Expression::Call(Box::new(Call {
                kind: types::CallKind::Unknown,
                receiver: Some(Expression::IdentifierRef(Box::new(
                    IdentifierRef {
                        kind: types::IdentifierKind::Unknown,
                        name: "a".to_string(),
                        usage: Usage::Used,
                        location: cols(8, 8)
                    }
                ))),
                name: Identifier {
                    name: "b".to_string(),
                    location: cols(10, 10)
                },
                parens: false,
                in_mut: false,
                usage: Usage::Used,
                arguments: Vec::new(),
                location: cols(8, 10)
            }))
        );
    }

    #[test]
    fn test_lower_builtin_call() {
        let hir = lower_expr("fn a { _INKO.foo(10) }").0;

        assert_eq!(
            hir,
            Expression::BuiltinCall(Box::new(BuiltinCall {
                info: None,
                name: Identifier {
                    name: "foo".to_string(),
                    location: cols(14, 16)
                },
                arguments: vec![Expression::Int(Box::new(IntLiteral {
                    value: 10,
                    resolved_type: types::TypeRef::Unknown,
                    location: cols(18, 19)
                }))],
                location: cols(8, 20)
            }))
        );
    }

    #[test]
    fn test_lower_builtin_call_outside_stdlib() {
        let name = ModuleName::new("foo");
        let ast =
            Parser::new("fn a { _INKO.foo(10) }".into(), "test.inko".into())
                .parse()
                .expect("Failed to parse the module");

        let ast = ParsedModule { ast, name };
        let mut state = State::new(Config::new());

        LowerToHir::run_all(&mut state, vec![ast]);

        assert_eq!(state.diagnostics.iter().count(), 1);
    }

    #[test]
    fn test_lower_builtin_call_with_named_arguments() {
        let (hir, diags) = lower_expr("fn a { _INKO.foo(a: 10) }");

        assert_eq!(diags, 1);
        assert_eq!(
            hir,
            Expression::BuiltinCall(Box::new(BuiltinCall {
                info: None,
                name: Identifier {
                    name: "foo".to_string(),
                    location: cols(14, 16)
                },
                arguments: vec![Expression::Int(Box::new(IntLiteral {
                    value: 10,
                    resolved_type: types::TypeRef::Unknown,
                    location: cols(21, 22)
                }))],
                location: cols(8, 23)
            }))
        );
    }

    #[test]
    fn test_lower_assign_variable() {
        let hir = lower_expr("fn a { a = 10 }").0;

        assert_eq!(
            hir,
            Expression::AssignVariable(Box::new(AssignVariable {
                variable_id: None,
                variable: Identifier {
                    name: "a".to_string(),
                    location: cols(8, 8)
                },
                value: Expression::Int(Box::new(IntLiteral {
                    value: 10,
                    resolved_type: types::TypeRef::Unknown,
                    location: cols(12, 13)
                })),
                resolved_type: types::TypeRef::Unknown,
                location: cols(8, 13)
            }))
        );
    }

    #[test]
    fn test_lower_replace_variable() {
        let hir = lower_expr("fn a { a =: 10 }").0;

        assert_eq!(
            hir,
            Expression::ReplaceVariable(Box::new(ReplaceVariable {
                variable_id: None,
                variable: Identifier {
                    name: "a".to_string(),
                    location: cols(8, 8)
                },
                value: Expression::Int(Box::new(IntLiteral {
                    value: 10,
                    resolved_type: types::TypeRef::Unknown,
                    location: cols(13, 14)
                })),
                resolved_type: types::TypeRef::Unknown,
                location: cols(8, 14)
            }))
        );
    }

    #[test]
    fn test_lower_assign_field() {
        let hir = lower_expr("fn a { @a = 10 }").0;

        assert_eq!(
            hir,
            Expression::AssignField(Box::new(AssignField {
                field_id: None,
                field: Field { name: "a".to_string(), location: cols(8, 9) },
                value: Expression::Int(Box::new(IntLiteral {
                    value: 10,
                    resolved_type: types::TypeRef::Unknown,
                    location: cols(13, 14)
                })),
                resolved_type: types::TypeRef::Unknown,
                location: cols(8, 14)
            }))
        );
    }

    #[test]
    fn test_lower_replace_field() {
        let hir = lower_expr("fn a { @a =: 10 }").0;

        assert_eq!(
            hir,
            Expression::ReplaceField(Box::new(ReplaceField {
                field_id: None,
                field: Field { name: "a".to_string(), location: cols(8, 9) },
                value: Expression::Int(Box::new(IntLiteral {
                    value: 10,
                    resolved_type: types::TypeRef::Unknown,
                    location: cols(14, 15)
                })),
                resolved_type: types::TypeRef::Unknown,
                location: cols(8, 15)
            }))
        );
    }

    #[test]
    fn test_lower_binary_assign_variable() {
        let hir = lower_expr("fn a { a += 1 }").0;

        assert_eq!(
            hir,
            Expression::AssignVariable(Box::new(AssignVariable {
                variable_id: None,
                variable: Identifier {
                    name: "a".to_string(),
                    location: cols(8, 8)
                },
                value: Expression::Call(Box::new(Call {
                    kind: types::CallKind::Unknown,
                    receiver: Some(Expression::IdentifierRef(Box::new(
                        IdentifierRef {
                            kind: types::IdentifierKind::Unknown,
                            name: "a".to_string(),
                            usage: Usage::Used,
                            location: cols(8, 8)
                        }
                    ))),
                    parens: true,
                    in_mut: false,
                    usage: Usage::Used,
                    arguments: vec![Argument::Positional(Box::new(
                        PositionalArgument {
                            value: Expression::Int(Box::new(IntLiteral {
                                value: 1,
                                resolved_type: types::TypeRef::Unknown,
                                location: cols(13, 13)
                            })),
                            expected_type: types::TypeRef::Unknown,
                        }
                    ))],
                    name: Identifier {
                        name: Operator::Add.method_name().to_string(),
                        location: cols(10, 11)
                    },
                    location: cols(8, 13)
                })),
                resolved_type: types::TypeRef::Unknown,
                location: cols(8, 13)
            }))
        );
    }

    #[test]
    fn test_lower_assign_setter() {
        let hir = lower_expr("fn a { a.b = 1 }").0;

        assert_eq!(
            hir,
            Expression::AssignSetter(Box::new(AssignSetter {
                kind: types::CallKind::Unknown,
                receiver: Expression::IdentifierRef(Box::new(IdentifierRef {
                    kind: types::IdentifierKind::Unknown,
                    name: "a".to_string(),
                    usage: Usage::Used,
                    location: cols(8, 8)
                })),
                name: Identifier {
                    name: "b".to_string(),
                    location: cols(10, 10)
                },
                value: Expression::Int(Box::new(IntLiteral {
                    resolved_type: types::TypeRef::Unknown,
                    value: 1,
                    location: cols(14, 14)
                })),
                location: cols(8, 14),
                usage: Usage::Used,
                expected_type: types::TypeRef::Unknown,
            }))
        );
    }

    #[test]
    fn test_lower_binary_assign_setter() {
        let hir = lower_expr("fn a { a.b += 1 }").0;

        assert_eq!(
            hir,
            Expression::AssignSetter(Box::new(AssignSetter {
                kind: types::CallKind::Unknown,
                receiver: Expression::IdentifierRef(Box::new(IdentifierRef {
                    kind: types::IdentifierKind::Unknown,
                    name: "a".to_string(),
                    usage: Usage::Used,
                    location: cols(8, 8)
                })),
                name: Identifier {
                    name: "b".to_string(),
                    location: cols(10, 10)
                },
                value: Expression::Call(Box::new(Call {
                    kind: types::CallKind::Unknown,
                    receiver: Some(Expression::Call(Box::new(Call {
                        kind: types::CallKind::Unknown,
                        receiver: Some(Expression::IdentifierRef(Box::new(
                            IdentifierRef {
                                kind: types::IdentifierKind::Unknown,
                                name: "a".to_string(),
                                usage: Usage::Used,
                                location: cols(8, 8)
                            }
                        ))),
                        name: Identifier {
                            name: "b".to_string(),
                            location: cols(10, 10)
                        },
                        parens: false,
                        in_mut: false,
                        usage: Usage::Used,
                        arguments: Vec::new(),
                        location: cols(8, 10)
                    }))),
                    parens: true,
                    in_mut: false,
                    usage: Usage::Used,
                    arguments: vec![Argument::Positional(Box::new(
                        PositionalArgument {
                            value: Expression::Int(Box::new(IntLiteral {
                                resolved_type: types::TypeRef::Unknown,
                                value: 1,
                                location: cols(15, 15)
                            })),
                            expected_type: types::TypeRef::Unknown,
                        }
                    ))],
                    name: Identifier {
                        name: Operator::Add.method_name().to_string(),
                        location: cols(12, 13)
                    },
                    location: cols(8, 15)
                })),
                location: cols(8, 15),
                usage: Usage::Used,
                expected_type: types::TypeRef::Unknown,
            }))
        );
    }

    #[test]
    fn test_lower_binary_assign_field() {
        let hir = lower_expr("fn a { @a += 1 }").0;

        assert_eq!(
            hir,
            Expression::AssignField(Box::new(AssignField {
                field_id: None,
                field: Field { name: "a".to_string(), location: cols(8, 9) },
                value: Expression::Call(Box::new(Call {
                    kind: types::CallKind::Unknown,
                    receiver: Some(Expression::FieldRef(Box::new(FieldRef {
                        info: None,
                        name: "a".to_string(),
                        in_mut: false,
                        location: cols(8, 9)
                    }))),
                    parens: true,
                    in_mut: false,
                    usage: Usage::Used,
                    arguments: vec![Argument::Positional(Box::new(
                        PositionalArgument {
                            value: Expression::Int(Box::new(IntLiteral {
                                value: 1,
                                resolved_type: types::TypeRef::Unknown,
                                location: cols(14, 14)
                            })),
                            expected_type: types::TypeRef::Unknown
                        }
                    ))],
                    name: Identifier {
                        name: Operator::Add.method_name().to_string(),
                        location: cols(11, 12)
                    },
                    location: cols(8, 14)
                })),
                resolved_type: types::TypeRef::Unknown,
                location: cols(8, 14)
            }))
        );
    }

    #[test]
    fn test_lower_closure() {
        let hir = lower_expr("fn a { fn (a: T) -> B { 10 } }").0;

        assert_eq!(
            hir,
            Expression::Closure(Box::new(Closure {
                closure_id: None,
                resolved_type: types::TypeRef::Unknown,
                moving: false,
                arguments: vec![BlockArgument {
                    variable_id: None,
                    name: Identifier {
                        name: "a".to_string(),
                        location: cols(12, 12)
                    },
                    value_type: Some(Type::Named(Box::new(TypeName {
                        self_type: false,
                        source: None,
                        resolved_type: types::TypeRef::Unknown,
                        name: Constant {
                            name: "T".to_string(),
                            location: cols(15, 15)
                        },
                        arguments: Vec::new(),
                        location: cols(15, 15)
                    }))),
                    location: cols(12, 15),
                }],
                return_type: Some(Type::Named(Box::new(TypeName {
                    self_type: false,
                    source: None,
                    resolved_type: types::TypeRef::Unknown,
                    name: Constant {
                        name: "B".to_string(),
                        location: cols(21, 21)
                    },
                    arguments: Vec::new(),
                    location: cols(21, 21)
                }))),
                body: vec![Expression::Int(Box::new(IntLiteral {
                    value: 10,
                    resolved_type: types::TypeRef::Unknown,
                    location: cols(25, 26)
                }))],
                location: cols(8, 28)
            }))
        );
    }

    #[test]
    fn test_lower_define_variable() {
        let hir = lower_expr("fn a { let mut a: T = 10 }").0;

        assert_eq!(
            hir,
            Expression::DefineVariable(Box::new(DefineVariable {
                resolved_type: types::TypeRef::Unknown,
                variable_id: None,
                name: Identifier {
                    name: "a".to_string(),
                    location: cols(16, 16)
                },
                mutable: true,
                value_type: Some(Type::Named(Box::new(TypeName {
                    self_type: false,
                    source: None,
                    resolved_type: types::TypeRef::Unknown,
                    name: Constant {
                        name: "T".to_string(),
                        location: cols(19, 19)
                    },
                    arguments: Vec::new(),
                    location: cols(19, 19)
                }))),
                value: Expression::Int(Box::new(IntLiteral {
                    value: 10,
                    resolved_type: types::TypeRef::Unknown,
                    location: cols(23, 24)
                })),
                location: cols(8, 24)
            }))
        );
    }

    #[test]
    fn test_lower_self_keyword() {
        let hir = lower_expr("fn a { self }").0;

        assert_eq!(
            hir,
            Expression::SelfObject(Box::new(SelfObject {
                resolved_type: types::TypeRef::Unknown,
                location: cols(8, 11)
            }))
        );
    }

    #[test]
    fn test_lower_group() {
        let hir = lower_expr("fn a { (10) }").0;

        assert_eq!(
            hir,
            Expression::Int(Box::new(IntLiteral {
                value: 10,
                resolved_type: types::TypeRef::Unknown,
                location: cols(9, 10)
            }))
        );
    }

    #[test]
    fn test_lower_next() {
        let hir = lower_expr("fn a { next }").0;

        assert_eq!(
            hir,
            Expression::Next(Box::new(Next { location: cols(8, 11) }))
        );
    }

    #[test]
    fn test_lower_break() {
        let hir = lower_expr("fn a { break }").0;

        assert_eq!(
            hir,
            Expression::Break(Box::new(Break { location: cols(8, 12) }))
        );
    }

    #[test]
    fn test_lower_nil_keyword() {
        let hir = lower_expr("fn a { nil }").0;

        assert_eq!(
            hir,
            Expression::Nil(Box::new(Nil {
                resolved_type: types::TypeRef::Unknown,
                location: cols(8, 10)
            }))
        );
    }

    #[test]
    fn test_lower_true_keyword() {
        let hir = lower_expr("fn a { true }").0;

        assert_eq!(
            hir,
            Expression::True(Box::new(True {
                resolved_type: types::TypeRef::Unknown,
                location: cols(8, 11)
            }))
        );
    }

    #[test]
    fn test_lower_false_keyword() {
        let hir = lower_expr("fn a { false }").0;

        assert_eq!(
            hir,
            Expression::False(Box::new(False {
                resolved_type: types::TypeRef::Unknown,
                location: cols(8, 12)
            }))
        );
    }

    #[test]
    fn test_lower_reference_expression() {
        let hir = lower_expr("fn a { ref 10 }").0;

        assert_eq!(
            hir,
            Expression::Ref(Box::new(Ref {
                resolved_type: types::TypeRef::Unknown,
                value: Expression::Int(Box::new(IntLiteral {
                    value: 10,
                    resolved_type: types::TypeRef::Unknown,
                    location: cols(12, 13)
                })),
                location: cols(8, 13)
            }))
        );
    }

    #[test]
    fn test_lower_mut_expression() {
        let hir = lower_expr("fn a { mut 10 }").0;

        assert_eq!(
            hir,
            Expression::Mut(Box::new(Mut {
                pointer_to_method: None,
                resolved_type: types::TypeRef::Unknown,
                value: Expression::Int(Box::new(IntLiteral {
                    value: 10,
                    resolved_type: types::TypeRef::Unknown,
                    location: cols(12, 13)
                })),
                location: cols(8, 13)
            }))
        );
    }

    #[test]
    fn test_lower_recover_expression() {
        let hir = lower_expr("fn a { recover 10 }").0;

        assert_eq!(
            hir,
            Expression::Recover(Box::new(Recover {
                resolved_type: types::TypeRef::Unknown,
                body: vec![Expression::Int(Box::new(IntLiteral {
                    value: 10,
                    resolved_type: types::TypeRef::Unknown,
                    location: cols(16, 17)
                }))],
                location: cols(8, 17)
            }))
        );
    }

    #[test]
    fn test_lower_and_expression() {
        let hir = lower_expr("fn a { 10 and 20 }").0;

        assert_eq!(
            hir,
            Expression::And(Box::new(And {
                resolved_type: types::TypeRef::Unknown,
                left: Expression::Int(Box::new(IntLiteral {
                    value: 10,
                    resolved_type: types::TypeRef::Unknown,
                    location: cols(8, 9)
                })),
                right: Expression::Int(Box::new(IntLiteral {
                    value: 20,
                    resolved_type: types::TypeRef::Unknown,
                    location: cols(15, 16)
                })),
                location: cols(8, 16)
            }))
        );
    }

    #[test]
    fn test_lower_or_expression() {
        let hir = lower_expr("fn a { 10 or 20 }").0;

        assert_eq!(
            hir,
            Expression::Or(Box::new(Or {
                resolved_type: types::TypeRef::Unknown,
                left: Expression::Int(Box::new(IntLiteral {
                    value: 10,
                    resolved_type: types::TypeRef::Unknown,
                    location: cols(8, 9)
                })),
                right: Expression::Int(Box::new(IntLiteral {
                    value: 20,
                    resolved_type: types::TypeRef::Unknown,
                    location: cols(14, 15)
                })),
                location: cols(8, 15)
            }))
        );
    }

    #[test]
    fn test_lower_type_cast() {
        let hir = lower_expr("fn a { 10 as T }").0;

        assert_eq!(
            hir,
            Expression::TypeCast(Box::new(TypeCast {
                resolved_type: types::TypeRef::Unknown,
                value: Expression::Int(Box::new(IntLiteral {
                    value: 10,
                    resolved_type: types::TypeRef::Unknown,
                    location: cols(8, 9)
                })),
                cast_to: Type::Named(Box::new(TypeName {
                    self_type: false,
                    source: None,
                    resolved_type: types::TypeRef::Unknown,
                    name: Constant {
                        name: "T".to_string(),
                        location: cols(14, 14)
                    },
                    arguments: Vec::new(),
                    location: cols(14, 14)
                })),
                location: cols(8, 14)
            }))
        );
    }

    #[test]
    fn test_lower_throw_expression() {
        let hir = lower_expr("fn a { throw 10 }").0;

        assert_eq!(
            hir,
            Expression::Throw(Box::new(Throw {
                resolved_type: types::TypeRef::Unknown,
                return_type: types::TypeRef::Unknown,
                value: Expression::Int(Box::new(IntLiteral {
                    value: 10,
                    resolved_type: types::TypeRef::Unknown,
                    location: cols(14, 15)
                })),
                location: cols(8, 15)
            }))
        );
    }

    #[test]
    fn test_lower_return_expression_with_value() {
        let hir = lower_expr("fn a { return 10 }").0;

        assert_eq!(
            hir,
            Expression::Return(Box::new(Return {
                resolved_type: types::TypeRef::Unknown,
                value: Some(Expression::Int(Box::new(IntLiteral {
                    value: 10,
                    resolved_type: types::TypeRef::Unknown,
                    location: cols(15, 16)
                }))),
                location: cols(8, 16)
            }))
        );
    }

    #[test]
    fn test_lower_return_expression_without_value() {
        let hir = lower_expr("fn a { return }").0;

        assert_eq!(
            hir,
            Expression::Return(Box::new(Return {
                resolved_type: types::TypeRef::Unknown,
                value: None,
                location: cols(8, 13)
            }))
        );
    }

    #[test]
    fn test_lower_try() {
        let hir = lower_expr("fn a { try a() }").0;

        assert_eq!(
            hir,
            Expression::Try(Box::new(Try {
                expression: Expression::Call(Box::new(Call {
                    kind: types::CallKind::Unknown,
                    receiver: None,
                    name: Identifier {
                        name: "a".to_string(),
                        location: cols(12, 12)
                    },
                    parens: true,
                    in_mut: false,
                    usage: Usage::Used,
                    arguments: Vec::new(),
                    location: cols(12, 14)
                })),
                kind: types::ThrowKind::Unknown,
                return_type: types::TypeRef::Unknown,
                location: cols(8, 14)
            }))
        );
    }

    #[test]
    fn test_lower_if_expression() {
        let hir =
            lower_expr("fn a { if 10 { 20 } else if 30 { 40 } else { 50 } }").0;

        assert_eq!(
            hir,
            Expression::Match(Box::new(Match {
                resolved_type: types::TypeRef::Unknown,
                expression: Expression::Int(Box::new(IntLiteral {
                    value: 10,
                    resolved_type: types::TypeRef::Unknown,
                    location: cols(11, 12)
                })),
                cases: vec![
                    MatchCase {
                        variable_ids: Vec::new(),
                        pattern: Pattern::True(Box::new(True {
                            resolved_type: types::TypeRef::Unknown,
                            location: cols(11, 12)
                        })),
                        guard: None,
                        body: vec![Expression::Int(Box::new(IntLiteral {
                            value: 20,
                            resolved_type: types::TypeRef::Unknown,
                            location: cols(16, 17)
                        }))],
                        location: cols(11, 19)
                    },
                    MatchCase {
                        variable_ids: Vec::new(),
                        pattern: Pattern::Wildcard(Box::new(WildcardPattern {
                            location: cols(29, 37)
                        })),
                        guard: Some(Expression::Int(Box::new(IntLiteral {
                            value: 30,
                            resolved_type: types::TypeRef::Unknown,
                            location: cols(29, 30)
                        }))),
                        body: vec![Expression::Int(Box::new(IntLiteral {
                            value: 40,
                            resolved_type: types::TypeRef::Unknown,
                            location: cols(34, 35)
                        }))],
                        location: cols(29, 37)
                    },
                    MatchCase {
                        variable_ids: Vec::new(),
                        pattern: Pattern::Wildcard(Box::new(WildcardPattern {
                            location: cols(44, 49)
                        })),
                        guard: None,
                        body: vec![Expression::Int(Box::new(IntLiteral {
                            value: 50,
                            resolved_type: types::TypeRef::Unknown,
                            location: cols(46, 47)
                        }))],
                        location: cols(44, 49)
                    }
                ],
                location: cols(8, 49),
                write_result: true
            }))
        );
    }

    #[test]
    fn test_lower_loop_expression() {
        let hir = lower_expr("fn a { loop { 10 } }").0;

        assert_eq!(
            hir,
            Expression::Loop(Box::new(Loop {
                body: vec![Expression::Int(Box::new(IntLiteral {
                    value: 10,
                    resolved_type: types::TypeRef::Unknown,
                    location: cols(15, 16)
                }))],
                location: cols(8, 18),
            }))
        );
    }

    #[test]
    fn test_lower_while_expression() {
        let hir = lower_expr("fn a { while 10 { 20 } }").0;

        assert_eq!(
            hir,
            Expression::Loop(Box::new(Loop {
                body: vec![Expression::Match(Box::new(Match {
                    resolved_type: types::TypeRef::Unknown,
                    expression: Expression::Int(Box::new(IntLiteral {
                        value: 10,
                        resolved_type: types::TypeRef::Unknown,
                        location: cols(14, 15)
                    })),
                    cases: vec![
                        MatchCase {
                            variable_ids: Vec::new(),
                            pattern: Pattern::True(Box::new(True {
                                resolved_type: types::TypeRef::Unknown,
                                location: cols(14, 15)
                            })),
                            guard: None,
                            body: vec![Expression::Int(Box::new(IntLiteral {
                                value: 20,
                                resolved_type: types::TypeRef::Unknown,
                                location: cols(19, 20)
                            }))],
                            location: cols(14, 15)
                        },
                        MatchCase {
                            variable_ids: Vec::new(),
                            pattern: Pattern::Wildcard(Box::new(
                                WildcardPattern { location: cols(14, 15) }
                            )),
                            guard: None,
                            body: vec![Expression::Break(Box::new(Break {
                                location: cols(14, 15)
                            }))],
                            location: cols(8, 22)
                        }
                    ],
                    location: cols(8, 22),
                    write_result: true,
                })),],
                location: cols(8, 22)
            }))
        );
    }

    #[test]
    fn test_lower_scope_expression() {
        let hir = lower_expr("fn a { { 10 } }").0;

        assert_eq!(
            hir,
            Expression::Scope(Box::new(Scope {
                resolved_type: types::TypeRef::Unknown,
                body: vec![Expression::Int(Box::new(IntLiteral {
                    value: 10,
                    resolved_type: types::TypeRef::Unknown,
                    location: cols(10, 11)
                }))],
                location: cols(8, 13)
            }))
        );
    }

    #[test]
    fn test_lower_match() {
        let hir = lower_expr("fn a { match 1 { case 1 if 2 -> 3 } }").0;

        assert_eq!(
            hir,
            Expression::Match(Box::new(Match {
                resolved_type: types::TypeRef::Unknown,
                expression: Expression::Int(Box::new(IntLiteral {
                    resolved_type: types::TypeRef::Unknown,
                    value: 1,
                    location: cols(14, 14)
                })),
                cases: vec![MatchCase {
                    variable_ids: Vec::new(),
                    pattern: Pattern::Int(Box::new(IntLiteral {
                        resolved_type: types::TypeRef::Unknown,
                        value: 1,
                        location: cols(23, 23)
                    })),
                    guard: Some(Expression::Int(Box::new(IntLiteral {
                        resolved_type: types::TypeRef::Unknown,
                        value: 2,
                        location: cols(28, 28)
                    }))),
                    body: vec![Expression::Int(Box::new(IntLiteral {
                        resolved_type: types::TypeRef::Unknown,
                        value: 3,
                        location: cols(33, 33)
                    }))],
                    location: cols(18, 33)
                }],
                location: cols(8, 35),
                write_result: true
            }))
        );
    }

    #[test]
    fn test_lower_enum_type() {
        let hir =
            lower_top_expr("type enum Option[T] { case Some(T) case None }").0;

        assert_eq!(
            hir,
            TopLevelExpression::Type(Box::new(DefineType {
                documentation: String::new(),
                public: false,
                semantics: TypeSemantics::Default,
                kind: TypeKind::Enum,
                type_id: None,
                name: Constant {
                    name: "Option".to_string(),
                    location: cols(11, 16),
                },
                type_parameters: vec![TypeParameter {
                    type_parameter_id: None,
                    name: Constant {
                        name: "T".to_string(),
                        location: cols(18, 18)
                    },
                    requirements: Vec::new(),
                    mutable: false,
                    copy: false,
                    location: cols(18, 18)
                }],
                body: vec![
                    TypeExpression::Constructor(Box::new(DefineConstructor {
                        documentation: String::new(),
                        method_id: None,
                        constructor_id: None,
                        name: Constant {
                            name: "Some".to_string(),
                            location: cols(28, 31)
                        },
                        members: vec![Type::Named(Box::new(TypeName {
                            self_type: false,
                            source: None,
                            resolved_type: types::TypeRef::Unknown,
                            name: Constant {
                                name: "T".to_string(),
                                location: cols(33, 33)
                            },
                            arguments: Vec::new(),
                            location: cols(33, 33)
                        }))],
                        location: cols(23, 34)
                    },)),
                    TypeExpression::Constructor(Box::new(DefineConstructor {
                        documentation: String::new(),
                        method_id: None,
                        constructor_id: None,
                        name: Constant {
                            name: "None".to_string(),
                            location: cols(41, 44)
                        },
                        members: Vec::new(),
                        location: cols(36, 39)
                    },))
                ],
                location: cols(1, 46)
            }))
        );
    }

    #[test]
    fn test_expression_is_recover() {
        let int = Expression::Int(Box::new(IntLiteral {
            value: 0,
            resolved_type: types::TypeRef::Unknown,
            location: cols(1, 1),
        }));

        let recover = Expression::Recover(Box::new(Recover {
            resolved_type: types::TypeRef::Unknown,
            body: vec![int.clone()],
            location: cols(1, 1),
        }));

        assert!(!int.is_recover());
        assert!(recover.is_recover());
    }
}
