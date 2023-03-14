//! Inko's high-level typed intermediate representation, or HIR for short.
//!
//! HIR is generated from the AST, and share many similarities with it. Unlike
//! the AST it stores type information, and some AST nodes are desugared into
//! different HIR nodes.
use crate::diagnostics::DiagnosticId;
use crate::modules_parser::ParsedModule;
use crate::state::State;
use ::ast::nodes::{self as ast, Node as _};
use ::ast::source_location::SourceLocation;
use std::path::PathBuf;
use std::str::FromStr;

const SET_INDEX_METHOD: &str = "set_index";
const BUILTIN_RECEIVER: &str = "_INKO";
const TRY_BINDING_VAR: &str = "$error";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct IntLiteral {
    pub(crate) value: i64,
    pub(crate) resolved_type: types::TypeRef,
    pub(crate) location: SourceLocation,
}

#[derive(Clone, Debug)]
pub(crate) struct FloatLiteral {
    pub(crate) value: f64,
    pub(crate) resolved_type: types::TypeRef,
    pub(crate) location: SourceLocation,
}

impl PartialEq for FloatLiteral {
    fn eq(&self, other: &Self) -> bool {
        // This is just to make unit testing easier.
        self.value == other.value && self.location == other.location
    }
}

impl Eq for FloatLiteral {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct StringText {
    pub(crate) value: String,
    pub(crate) location: SourceLocation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum StringValue {
    Text(Box<StringText>),
    Expression(Box<Call>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct StringLiteral {
    pub(crate) values: Vec<StringValue>,
    pub(crate) resolved_type: types::TypeRef,
    pub(crate) location: SourceLocation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ConstStringLiteral {
    pub(crate) value: String,
    pub(crate) resolved_type: types::TypeRef,
    pub(crate) location: SourceLocation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ArrayLiteral {
    pub(crate) value_type: types::TypeRef,
    pub(crate) resolved_type: types::TypeRef,
    pub(crate) values: Vec<Expression>,
    pub(crate) location: SourceLocation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TupleLiteral {
    pub(crate) class_id: Option<types::ClassId>,
    pub(crate) value_types: Vec<types::TypeRef>,
    pub(crate) resolved_type: types::TypeRef,
    pub(crate) values: Vec<Expression>,
    pub(crate) location: SourceLocation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Identifier {
    pub(crate) name: String,
    pub(crate) location: SourceLocation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Constant {
    pub(crate) name: String,
    pub(crate) location: SourceLocation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ConstantRef {
    pub(crate) kind: types::ConstantKind,
    pub(crate) source: Option<Identifier>,
    pub(crate) name: String,
    pub(crate) resolved_type: types::TypeRef,
    pub(crate) location: SourceLocation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct IdentifierRef {
    pub(crate) name: String,
    pub(crate) kind: types::IdentifierKind,
    pub(crate) location: SourceLocation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Call {
    pub(crate) kind: types::CallKind,
    pub(crate) receiver: Option<Expression>,
    pub(crate) name: Identifier,
    pub(crate) arguments: Vec<Argument>,
    pub(crate) else_block: Option<ElseBlock>,
    pub(crate) location: SourceLocation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BuiltinCall {
    pub(crate) info: Option<types::BuiltinCallInfo>,
    pub(crate) name: Identifier,
    pub(crate) arguments: Vec<Expression>,
    pub(crate) else_block: Option<ElseBlock>,
    pub(crate) location: SourceLocation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AssignField {
    pub(crate) field_id: Option<types::FieldId>,
    pub(crate) resolved_type: types::TypeRef,
    pub(crate) field: Field,
    pub(crate) value: Expression,
    pub(crate) location: SourceLocation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ReplaceField {
    pub(crate) field_id: Option<types::FieldId>,
    pub(crate) resolved_type: types::TypeRef,
    pub(crate) field: Field,
    pub(crate) value: Expression,
    pub(crate) location: SourceLocation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AssignVariable {
    pub(crate) variable_id: Option<types::VariableId>,
    pub(crate) resolved_type: types::TypeRef,
    pub(crate) variable: Identifier,
    pub(crate) value: Expression,
    pub(crate) location: SourceLocation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ReplaceVariable {
    pub(crate) variable_id: Option<types::VariableId>,
    pub(crate) resolved_type: types::TypeRef,
    pub(crate) variable: Identifier,
    pub(crate) value: Expression,
    pub(crate) location: SourceLocation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AssignSetter {
    pub(crate) kind: types::CallKind,
    pub(crate) receiver: Expression,
    pub(crate) name: Identifier,
    pub(crate) value: Expression,
    pub(crate) else_block: Option<ElseBlock>,
    pub(crate) location: SourceLocation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ImportSymbol {
    pub(crate) name: Identifier,
    pub(crate) import_as: Identifier,
    pub(crate) location: SourceLocation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Import {
    pub(crate) source: Vec<Identifier>,
    pub(crate) symbols: Vec<ImportSymbol>,
    pub(crate) location: SourceLocation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DefineConstant {
    pub(crate) public: bool,
    pub(crate) constant_id: Option<types::ConstantId>,
    pub(crate) name: Constant,
    pub(crate) value: ConstExpression,
    pub(crate) location: SourceLocation,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum MethodKind {
    Regular,
    Moving,
    Mutable,
}

impl MethodKind {
    pub(crate) fn is_moving(self) -> bool {
        self == Self::Moving
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DefineInstanceMethod {
    pub(crate) public: bool,
    pub(crate) kind: MethodKind,
    pub(crate) name: Identifier,
    pub(crate) type_parameters: Vec<TypeParameter>,
    pub(crate) arguments: Vec<MethodArgument>,
    pub(crate) throw_type: Option<Type>,
    pub(crate) return_type: Option<Type>,
    pub(crate) body: Vec<Expression>,
    pub(crate) location: SourceLocation,
    pub(crate) method_id: Option<types::MethodId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DefineModuleMethod {
    pub(crate) public: bool,
    pub(crate) name: Identifier,
    pub(crate) type_parameters: Vec<TypeParameter>,
    pub(crate) arguments: Vec<MethodArgument>,
    pub(crate) throw_type: Option<Type>,
    pub(crate) return_type: Option<Type>,
    pub(crate) body: Vec<Expression>,
    pub(crate) location: SourceLocation,
    pub(crate) method_id: Option<types::MethodId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DefineRequiredMethod {
    pub(crate) public: bool,
    pub(crate) kind: MethodKind,
    pub(crate) name: Identifier,
    pub(crate) type_parameters: Vec<TypeParameter>,
    pub(crate) arguments: Vec<MethodArgument>,
    pub(crate) throw_type: Option<Type>,
    pub(crate) return_type: Option<Type>,
    pub(crate) method_id: Option<types::MethodId>,
    pub(crate) location: SourceLocation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DefineStaticMethod {
    pub(crate) public: bool,
    pub(crate) name: Identifier,
    pub(crate) type_parameters: Vec<TypeParameter>,
    pub(crate) arguments: Vec<MethodArgument>,
    pub(crate) throw_type: Option<Type>,
    pub(crate) return_type: Option<Type>,
    pub(crate) body: Vec<Expression>,
    pub(crate) location: SourceLocation,
    pub(crate) method_id: Option<types::MethodId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DefineAsyncMethod {
    pub(crate) mutable: bool,
    pub(crate) public: bool,
    pub(crate) name: Identifier,
    pub(crate) type_parameters: Vec<TypeParameter>,
    pub(crate) arguments: Vec<MethodArgument>,
    pub(crate) throw_type: Option<Type>,
    pub(crate) return_type: Option<Type>,
    pub(crate) body: Vec<Expression>,
    pub(crate) location: SourceLocation,
    pub(crate) method_id: Option<types::MethodId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DefineField {
    pub(crate) public: bool,
    pub(crate) field_id: Option<types::FieldId>,
    pub(crate) name: Identifier,
    pub(crate) value_type: Type,
    pub(crate) location: SourceLocation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ClassExpression {
    InstanceMethod(Box<DefineInstanceMethod>),
    StaticMethod(Box<DefineStaticMethod>),
    AsyncMethod(Box<DefineAsyncMethod>),
    Field(Box<DefineField>),
    Variant(Box<DefineVariant>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ClassKind {
    Async,
    Builtin,
    Enum,
    Regular,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DefineClass {
    pub(crate) public: bool,
    pub(crate) class_id: Option<types::ClassId>,
    pub(crate) kind: ClassKind,
    pub(crate) name: Constant,
    pub(crate) type_parameters: Vec<TypeParameter>,
    pub(crate) body: Vec<ClassExpression>,
    pub(crate) location: SourceLocation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DefineVariant {
    pub(crate) method_id: Option<types::MethodId>,
    pub(crate) variant_id: Option<types::VariantId>,
    pub(crate) name: Constant,
    pub(crate) members: Vec<Type>,
    pub(crate) location: SourceLocation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AssignInstanceLiteralField {
    pub(crate) resolved_type: types::TypeRef,
    pub(crate) field_id: Option<types::FieldId>,
    pub(crate) field: Field,
    pub(crate) value: Expression,
    pub(crate) location: SourceLocation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ClassLiteral {
    pub(crate) class_id: Option<types::ClassId>,
    pub(crate) resolved_type: types::TypeRef,
    pub(crate) class_name: Constant,
    pub(crate) fields: Vec<AssignInstanceLiteralField>,
    pub(crate) location: SourceLocation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum TraitExpression {
    InstanceMethod(Box<DefineInstanceMethod>),
    RequiredMethod(Box<DefineRequiredMethod>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DefineTrait {
    pub(crate) public: bool,
    pub(crate) trait_id: Option<types::TraitId>,
    pub(crate) name: Constant,
    pub(crate) type_parameters: Vec<TypeParameter>,
    pub(crate) requirements: Vec<TypeName>,
    pub(crate) body: Vec<TraitExpression>,
    pub(crate) location: SourceLocation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum TopLevelExpression {
    Class(Box<DefineClass>),
    Constant(Box<DefineConstant>),
    ModuleMethod(Box<DefineModuleMethod>),
    Trait(Box<DefineTrait>),
    Implement(Box<ImplementTrait>),
    Import(Box<Import>),
    Reopen(Box<ReopenClass>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ReopenClass {
    pub(crate) class_id: Option<types::ClassId>,
    pub(crate) class_name: Constant,
    pub(crate) body: Vec<ReopenClassExpression>,
    pub(crate) bounds: Vec<TypeBound>,
    pub(crate) location: SourceLocation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ReopenClassExpression {
    InstanceMethod(Box<DefineInstanceMethod>),
    StaticMethod(Box<DefineStaticMethod>),
    AsyncMethod(Box<DefineAsyncMethod>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Requirement {
    Trait(TypeName),
    Mutable(SourceLocation),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TypeBound {
    pub(crate) name: Constant,
    pub(crate) requirements: Vec<Requirement>,
    pub(crate) location: SourceLocation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ImplementTrait {
    pub(crate) trait_name: TypeName,
    pub(crate) class_name: Constant,
    pub(crate) body: Vec<DefineInstanceMethod>,
    pub(crate) location: SourceLocation,
    pub(crate) bounds: Vec<TypeBound>,
    pub(crate) trait_instance: Option<types::TraitInstance>,
    pub(crate) class_instance: Option<types::ClassInstance>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Scope {
    pub(crate) resolved_type: types::TypeRef,
    pub(crate) body: Vec<Expression>,
    pub(crate) location: SourceLocation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Index {
    pub(crate) info: Option<types::CallInfo>,
    pub(crate) receiver: Expression,
    pub(crate) index: Expression,
    pub(crate) location: SourceLocation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Expression {
    And(Box<And>),
    Array(Box<ArrayLiteral>),
    AssignField(Box<AssignField>),
    ReplaceField(Box<ReplaceField>),
    AssignSetter(Box<AssignSetter>),
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
    Index(Box<Index>),
    ClassLiteral(Box<ClassLiteral>),
    Int(Box<IntLiteral>),
    Invalid(Box<SourceLocation>),
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
}

impl Expression {
    pub(crate) fn location(&self) -> &SourceLocation {
        match self {
            Expression::And(ref n) => &n.location,
            Expression::Array(ref n) => &n.location,
            Expression::AssignField(ref n) => &n.location,
            Expression::ReplaceField(ref n) => &n.location,
            Expression::AssignSetter(ref n) => &n.location,
            Expression::AssignVariable(ref n) => &n.location,
            Expression::ReplaceVariable(ref n) => &n.location,
            Expression::Break(ref n) => &n.location,
            Expression::BuiltinCall(ref n) => &n.location,
            Expression::Call(ref n) => &n.location,
            Expression::Closure(ref n) => &n.location,
            Expression::ConstantRef(ref n) => &n.location,
            Expression::DefineVariable(ref n) => &n.location,
            Expression::False(ref n) => &n.location,
            Expression::FieldRef(ref n) => &n.location,
            Expression::Float(ref n) => &n.location,
            Expression::IdentifierRef(ref n) => &n.location,
            Expression::Index(ref n) => &n.location,
            Expression::ClassLiteral(ref n) => &n.location,
            Expression::Int(ref n) => &n.location,
            Expression::Invalid(ref n) => n,
            Expression::Loop(ref n) => &n.location,
            Expression::Match(ref n) => &n.location,
            Expression::Mut(ref n) => &n.location,
            Expression::Next(ref n) => &n.location,
            Expression::Or(ref n) => &n.location,
            Expression::Ref(ref n) => &n.location,
            Expression::Return(ref n) => &n.location,
            Expression::Scope(ref n) => &n.location,
            Expression::SelfObject(ref n) => &n.location,
            Expression::String(ref n) => &n.location,
            Expression::Throw(ref n) => &n.location,
            Expression::True(ref n) => &n.location,
            Expression::Nil(ref n) => &n.location,
            Expression::Tuple(ref n) => &n.location,
            Expression::TypeCast(ref n) => &n.location,
            Expression::Recover(ref n) => &n.location,
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
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ConstExpression {
    Int(Box<IntLiteral>),
    String(Box<ConstStringLiteral>),
    Float(Box<FloatLiteral>),
    Binary(Box<ConstBinary>),
    ConstantRef(Box<ConstantRef>),
    Array(Box<ConstArray>),
    Invalid(Box<SourceLocation>),
}

impl ConstExpression {
    pub(crate) fn location(&self) -> &SourceLocation {
        match self {
            Self::Int(ref n) => &n.location,
            Self::String(ref n) => &n.location,
            Self::Float(ref n) => &n.location,
            Self::Binary(ref n) => &n.location,
            Self::ConstantRef(ref n) => &n.location,
            Self::Array(ref n) => &n.location,
            Self::Invalid(ref l) => l,
        }
    }

    pub(crate) fn is_simple_literal(&self) -> bool {
        matches!(
            self,
            ConstExpression::Int(_)
                | ConstExpression::Float(_)
                | ConstExpression::String(_)
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TypeParameter {
    pub(crate) type_parameter_id: Option<types::TypeParameterId>,
    pub(crate) name: Constant,
    pub(crate) requirements: Vec<TypeName>,
    pub(crate) location: SourceLocation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MethodArgument {
    pub(crate) name: Identifier,
    pub(crate) value_type: Type,
    pub(crate) location: SourceLocation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct NamedArgument {
    pub(crate) index: usize,
    pub(crate) name: Identifier,
    pub(crate) value: Expression,
    pub(crate) location: SourceLocation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Argument {
    Positional(Box<Expression>),
    Named(Box<NamedArgument>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TypeName {
    pub(crate) source: Option<Identifier>,
    pub(crate) resolved_type: types::TypeRef,
    pub(crate) name: Constant,
    pub(crate) arguments: Vec<Type>,
    pub(crate) location: SourceLocation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ReferenceType {
    pub(crate) type_reference: ReferrableType,
    pub(crate) location: SourceLocation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RcType {
    pub(crate) name: TypeName,
    pub(crate) location: SourceLocation,
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
    pub(crate) throw_type: Option<Type>,
    pub(crate) return_type: Option<Type>,
    pub(crate) location: SourceLocation,
    pub(crate) resolved_type: types::TypeRef,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TupleType {
    pub(crate) resolved_type: types::TypeRef,
    pub(crate) values: Vec<Type>,
    pub(crate) location: SourceLocation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Type {
    Named(Box<TypeName>),
    Ref(Box<ReferenceType>),
    Mut(Box<ReferenceType>),
    Uni(Box<ReferenceType>),
    Closure(Box<ClosureType>),
    Tuple(Box<TupleType>),
}

impl Type {
    pub(crate) fn location(&self) -> &SourceLocation {
        match self {
            Type::Named(ref node) => &node.location,
            Type::Ref(ref node) => &node.location,
            Type::Mut(ref node) => &node.location,
            Type::Uni(ref node) => &node.location,
            Type::Closure(ref node) => &node.location,
            Type::Tuple(ref node) => &node.location,
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
    pub(crate) location: SourceLocation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ConstArray {
    pub(crate) resolved_type: types::TypeRef,
    pub(crate) values: Vec<ConstExpression>,
    pub(crate) location: SourceLocation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Field {
    pub(crate) name: String,
    pub(crate) location: SourceLocation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FieldRef {
    pub(crate) field_id: Option<types::FieldId>,
    pub(crate) name: String,
    pub(crate) resolved_type: types::TypeRef,
    pub(crate) location: SourceLocation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BlockArgument {
    pub(crate) variable_id: Option<types::VariableId>,
    pub(crate) name: Identifier,
    pub(crate) value_type: Option<Type>,
    pub(crate) location: SourceLocation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Closure {
    pub(crate) closure_id: Option<types::ClosureId>,
    pub(crate) resolved_type: types::TypeRef,
    pub(crate) moving: bool,
    pub(crate) arguments: Vec<BlockArgument>,
    pub(crate) throw_type: Option<Type>,
    pub(crate) return_type: Option<Type>,
    pub(crate) body: Vec<Expression>,
    pub(crate) location: SourceLocation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DefineElse {
    pub(crate) body: Vec<Expression>,
    pub(crate) location: SourceLocation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DefineVariable {
    pub(crate) resolved_type: types::TypeRef,
    pub(crate) variable_id: Option<types::VariableId>,
    pub(crate) mutable: bool,
    pub(crate) name: Identifier,
    pub(crate) value_type: Option<Type>,
    pub(crate) value: Expression,
    pub(crate) location: SourceLocation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SelfObject {
    pub(crate) resolved_type: types::TypeRef,
    pub(crate) location: SourceLocation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct True {
    pub(crate) resolved_type: types::TypeRef,
    pub(crate) location: SourceLocation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Nil {
    pub(crate) resolved_type: types::TypeRef,
    pub(crate) location: SourceLocation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct False {
    pub(crate) resolved_type: types::TypeRef,
    pub(crate) location: SourceLocation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Next {
    pub(crate) location: SourceLocation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Break {
    pub(crate) location: SourceLocation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Ref {
    pub(crate) resolved_type: types::TypeRef,
    pub(crate) value: Expression,
    pub(crate) location: SourceLocation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Mut {
    pub(crate) resolved_type: types::TypeRef,
    pub(crate) value: Expression,
    pub(crate) location: SourceLocation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Recover {
    pub(crate) resolved_type: types::TypeRef,
    pub(crate) body: Vec<Expression>,
    pub(crate) location: SourceLocation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct And {
    pub(crate) resolved_type: types::TypeRef,
    pub(crate) left: Expression,
    pub(crate) right: Expression,
    pub(crate) location: SourceLocation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Or {
    pub(crate) resolved_type: types::TypeRef,
    pub(crate) left: Expression,
    pub(crate) right: Expression,
    pub(crate) location: SourceLocation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TypeCast {
    pub(crate) resolved_type: types::TypeRef,
    pub(crate) value: Expression,
    pub(crate) cast_to: Type,
    pub(crate) location: SourceLocation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Throw {
    pub(crate) resolved_type: types::TypeRef,
    pub(crate) value: Expression,
    pub(crate) location: SourceLocation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Return {
    pub(crate) resolved_type: types::TypeRef,
    pub(crate) value: Option<Expression>,
    pub(crate) location: SourceLocation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ElseBlock {
    pub(crate) body: Vec<Expression>,
    pub(crate) argument: Option<BlockArgument>,
    pub(crate) location: SourceLocation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TuplePattern {
    pub(crate) field_ids: Vec<types::FieldId>,
    pub(crate) values: Vec<Pattern>,
    pub(crate) location: SourceLocation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FieldPattern {
    pub(crate) field_id: Option<types::FieldId>,
    pub(crate) field: Field,
    pub(crate) pattern: Pattern,
    pub(crate) location: SourceLocation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ClassPattern {
    pub(crate) class_id: Option<types::ClassId>,
    pub(crate) values: Vec<FieldPattern>,
    pub(crate) location: SourceLocation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct VariantPattern {
    pub(crate) variant_id: Option<types::VariantId>,
    pub(crate) name: Constant,
    pub(crate) values: Vec<Pattern>,
    pub(crate) location: SourceLocation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct WildcardPattern {
    pub(crate) location: SourceLocation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct IdentifierPattern {
    pub(crate) variable_id: Option<types::VariableId>,
    pub(crate) name: Identifier,
    pub(crate) mutable: bool,
    pub(crate) value_type: Option<Type>,
    pub(crate) location: SourceLocation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ConstantPattern {
    pub(crate) kind: types::ConstantPatternKind,
    pub(crate) source: Option<Identifier>,
    pub(crate) name: String,
    pub(crate) location: SourceLocation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct OrPattern {
    pub(crate) patterns: Vec<Pattern>,
    pub(crate) location: SourceLocation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StringPattern {
    pub value: String,
    pub location: SourceLocation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Pattern {
    Class(Box<ClassPattern>),
    Constant(Box<ConstantPattern>),
    Identifier(Box<IdentifierPattern>),
    Int(Box<IntLiteral>),
    String(Box<StringPattern>),
    Tuple(Box<TuplePattern>),
    Variant(Box<VariantPattern>),
    Wildcard(Box<WildcardPattern>),
    True(Box<True>),
    False(Box<False>),
    Or(Box<OrPattern>),
}

impl Pattern {
    pub(crate) fn location(&self) -> &SourceLocation {
        match self {
            Pattern::Constant(ref n) => &n.location,
            Pattern::Variant(ref n) => &n.location,
            Pattern::Int(ref n) => &n.location,
            Pattern::String(ref n) => &n.location,
            Pattern::Identifier(ref n) => &n.location,
            Pattern::Tuple(ref n) => &n.location,
            Pattern::Class(ref n) => &n.location,
            Pattern::Wildcard(ref n) => &n.location,
            Pattern::True(ref n) => &n.location,
            Pattern::False(ref n) => &n.location,
            Pattern::Or(ref n) => &n.location,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MatchCase {
    pub(crate) variable_ids: Vec<types::VariableId>,
    pub(crate) pattern: Pattern,
    pub(crate) guard: Option<Expression>,
    pub(crate) body: Vec<Expression>,
    pub(crate) location: SourceLocation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Match {
    pub(crate) resolved_type: types::TypeRef,
    pub(crate) expression: Expression,
    pub(crate) cases: Vec<MatchCase>,
    pub(crate) location: SourceLocation,
    pub(crate) write_result: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Loop {
    pub(crate) body: Vec<Expression>,
    pub(crate) location: SourceLocation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Module {
    pub(crate) module_id: types::ModuleId,
    pub(crate) expressions: Vec<TopLevelExpression>,
    pub(crate) location: SourceLocation,
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
        let expressions = self.top_level_expressions(module.ast.expressions);
        let location = module.ast.location;

        Module { module_id: self.module, expressions, location }
    }

    fn file(&self) -> PathBuf {
        self.module.file(&self.state.db)
    }

    fn top_level_expressions(
        &mut self,
        nodes: Vec<ast::TopLevelExpression>,
    ) -> Vec<TopLevelExpression> {
        nodes
            .into_iter()
            .map(|node| match node {
                ast::TopLevelExpression::DefineConstant(node) => {
                    self.define_constant(*node)
                }
                ast::TopLevelExpression::DefineMethod(node) => {
                    self.define_module_method(*node)
                }
                ast::TopLevelExpression::DefineClass(node) => {
                    self.define_class(*node)
                }
                ast::TopLevelExpression::DefineTrait(node) => {
                    self.define_trait(*node)
                }
                ast::TopLevelExpression::ReopenClass(node) => {
                    self.reopen_class(*node)
                }
                ast::TopLevelExpression::ImplementTrait(node) => {
                    self.implement_trait(*node)
                }
                ast::TopLevelExpression::Import(node) => self.import(*node),
            })
            .collect()
    }

    fn define_constant(
        &mut self,
        node: ast::DefineConstant,
    ) -> TopLevelExpression {
        let node = DefineConstant {
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
    ) -> TopLevelExpression {
        self.operator_method_not_allowed(node.operator, &node.location);

        TopLevelExpression::ModuleMethod(Box::new(DefineModuleMethod {
            public: node.public,
            name: self.identifier(node.name),
            type_parameters: self
                .optional_type_parameters(node.type_parameters),
            arguments: self.optional_method_arguments(node.arguments),
            throw_type: node.throw_type.map(|n| self.type_reference(n)),
            return_type: node.return_type.map(|n| self.type_reference(n)),
            body: self.optional_expressions(node.body),
            method_id: None,
            location: node.location,
        }))
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

    fn define_class(&mut self, node: ast::DefineClass) -> TopLevelExpression {
        TopLevelExpression::Class(Box::new(DefineClass {
            public: node.public,
            class_id: None,
            kind: match node.kind {
                ast::ClassKind::Async => ClassKind::Async,
                ast::ClassKind::Enum => ClassKind::Enum,
                ast::ClassKind::Builtin => ClassKind::Builtin,
                _ => ClassKind::Regular,
            },
            name: self.constant(node.name),
            type_parameters: self
                .optional_type_parameters(node.type_parameters),
            body: self.class_expressions(node.body),
            location: node.location,
        }))
    }

    fn class_expressions(
        &mut self,
        node: ast::ClassExpressions,
    ) -> Vec<ClassExpression> {
        node.values
            .into_iter()
            .map(|n| match n {
                ast::ClassExpression::DefineMethod(node) => {
                    self.define_method_in_class(*node)
                }
                ast::ClassExpression::DefineField(node) => {
                    self.define_field(*node)
                }
                ast::ClassExpression::DefineVariant(node) => {
                    self.define_case(*node)
                }
            })
            .collect()
    }

    fn define_field(&self, node: ast::DefineField) -> ClassExpression {
        ClassExpression::Field(Box::new(DefineField {
            public: node.public,
            field_id: None,
            name: self.identifier(node.name),
            value_type: self.type_reference(node.value_type),
            location: node.location,
        }))
    }

    fn define_case(&mut self, node: ast::DefineVariant) -> ClassExpression {
        ClassExpression::Variant(Box::new(DefineVariant {
            method_id: None,
            variant_id: None,
            name: self.constant(node.name),
            members: self.optional_types(node.members),
            location: node.location,
        }))
    }

    fn define_method_in_class(
        &mut self,
        node: ast::DefineMethod,
    ) -> ClassExpression {
        match node.kind {
            ast::MethodKind::Async | ast::MethodKind::AsyncMutable => {
                ClassExpression::AsyncMethod(self.define_async_method(node))
            }
            ast::MethodKind::Static => {
                ClassExpression::StaticMethod(self.define_static_method(node))
            }
            _ => ClassExpression::InstanceMethod(Box::new(
                self.define_instance_method(node),
            )),
        }
    }

    fn define_static_method(
        &mut self,
        node: ast::DefineMethod,
    ) -> Box<DefineStaticMethod> {
        self.operator_method_not_allowed(node.operator, &node.location);

        Box::new(DefineStaticMethod {
            public: node.public,
            name: self.identifier(node.name),
            type_parameters: self
                .optional_type_parameters(node.type_parameters),
            arguments: self.optional_method_arguments(node.arguments),
            throw_type: node.throw_type.map(|n| self.type_reference(n)),
            return_type: node.return_type.map(|n| self.type_reference(n)),
            body: self.optional_expressions(node.body),
            method_id: None,
            location: node.location,
        })
    }

    fn define_async_method(
        &mut self,
        node: ast::DefineMethod,
    ) -> Box<DefineAsyncMethod> {
        self.operator_method_not_allowed(node.operator, &node.location);

        Box::new(DefineAsyncMethod {
            mutable: node.kind == ast::MethodKind::AsyncMutable,
            public: node.public,
            name: self.identifier(node.name),
            type_parameters: self
                .optional_type_parameters(node.type_parameters),
            arguments: self.optional_method_arguments(node.arguments),
            throw_type: node.throw_type.map(|n| self.type_reference(n)),
            return_type: node.return_type.map(|n| self.type_reference(n)),
            body: self.optional_expressions(node.body),
            method_id: None,
            location: node.location,
        })
    }

    fn define_instance_method(
        &mut self,
        node: ast::DefineMethod,
    ) -> DefineInstanceMethod {
        self.check_operator_requirements(&node);

        DefineInstanceMethod {
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
            throw_type: node.throw_type.map(|n| self.type_reference(n)),
            return_type: node.return_type.map(|n| self.type_reference(n)),
            body: self.optional_expressions(node.body),
            method_id: None,
            location: node.location,
        }
    }

    fn define_required_method(
        &mut self,
        node: ast::DefineMethod,
    ) -> Box<DefineRequiredMethod> {
        self.check_operator_requirements(&node);

        Box::new(DefineRequiredMethod {
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
            throw_type: node.throw_type.map(|n| self.type_reference(n)),
            return_type: node.return_type.map(|n| self.type_reference(n)),
            method_id: None,
            location: node.location,
        })
    }

    fn optional_type_bounds(
        &self,
        node: Option<ast::TypeBounds>,
    ) -> Vec<TypeBound> {
        if let Some(types) = node {
            types.values.into_iter().map(|n| self.type_bound(n)).collect()
        } else {
            Vec::new()
        }
    }

    fn type_bound(&self, node: ast::TypeBound) -> TypeBound {
        TypeBound {
            name: self.constant(node.name),
            requirements: self.requirements(node.requirements),
            location: node.location,
        }
    }

    fn requirements(&self, node: ast::Requirements) -> Vec<Requirement> {
        node.values
            .into_iter()
            .map(|node| match node {
                ast::Requirement::Trait(n) => {
                    Requirement::Trait(self.type_name(n))
                }
                ast::Requirement::Mutable(loc) => Requirement::Mutable(loc),
            })
            .collect()
    }

    fn define_trait(&mut self, node: ast::DefineTrait) -> TopLevelExpression {
        TopLevelExpression::Trait(Box::new(DefineTrait {
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
        node.values
            .into_iter()
            .map(|node| self.define_method_in_trait(node))
            .collect()
    }

    fn define_method_in_trait(
        &mut self,
        node: ast::DefineMethod,
    ) -> TraitExpression {
        if node.body.is_some() {
            TraitExpression::InstanceMethod(Box::new(
                self.define_instance_method(node),
            ))
        } else {
            TraitExpression::RequiredMethod(self.define_required_method(node))
        }
    }

    fn reopen_class(&mut self, node: ast::ReopenClass) -> TopLevelExpression {
        TopLevelExpression::Reopen(Box::new(ReopenClass {
            class_id: None,
            class_name: self.constant(node.class_name),
            body: self.reopen_class_expressions(node.body),
            bounds: self.optional_type_bounds(node.bounds),
            location: node.location,
        }))
    }

    fn reopen_class_expressions(
        &mut self,
        nodes: ast::ImplementationExpressions,
    ) -> Vec<ReopenClassExpression> {
        nodes
            .values
            .into_iter()
            .map(|node| self.define_method_in_reopen_class(node))
            .collect()
    }

    fn define_method_in_reopen_class(
        &mut self,
        node: ast::DefineMethod,
    ) -> ReopenClassExpression {
        match node.kind {
            ast::MethodKind::Static => ReopenClassExpression::StaticMethod(
                self.define_static_method(node),
            ),
            ast::MethodKind::Async => ReopenClassExpression::AsyncMethod(
                self.define_async_method(node),
            ),
            _ => ReopenClassExpression::InstanceMethod(Box::new(
                self.define_instance_method(node),
            )),
        }
    }

    fn implement_trait(
        &mut self,
        node: ast::ImplementTrait,
    ) -> TopLevelExpression {
        TopLevelExpression::Implement(Box::new(ImplementTrait {
            trait_name: self.type_name(node.trait_name),
            class_name: self.constant(node.class_name),
            bounds: self.optional_type_bounds(node.bounds),
            body: self.trait_implementation_expressions(node.body),
            location: node.location,
            trait_instance: None,
            class_instance: None,
        }))
    }

    fn trait_implementation_expressions(
        &mut self,
        node: ast::ImplementationExpressions,
    ) -> Vec<DefineInstanceMethod> {
        node.values
            .into_iter()
            .map(|n| self.define_instance_method(n))
            .collect()
    }

    fn import(&self, node: ast::Import) -> TopLevelExpression {
        TopLevelExpression::Import(Box::new(Import {
            source: self.import_module_path(node.path),
            symbols: self.import_symbols(node.symbols),
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
                let name = Identifier {
                    name: symbol.name,
                    location: symbol.location.clone(),
                };

                let import_as = if let Some(n) = symbol.alias {
                    Identifier { name: n.name, location: n.location }
                } else {
                    name.clone()
                };

                let location = SourceLocation::start_end(
                    &name.location,
                    &import_as.location,
                );

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
            throw_type: node.throw_type.map(|n| self.type_reference(n)),
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
        let requirements = self.optional_type_names(node.requirements);

        TypeParameter { type_parameter_id: None, name, requirements, location }
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
            ast::Expression::SingleString(node) => {
                ConstExpression::String(self.const_single_string_literal(*node))
            }
            ast::Expression::DoubleString(node) => {
                self.const_double_string_literal(*node)
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
            node => {
                self.state.diagnostics.error(
                    DiagnosticId::InvalidConstExpr,
                    "Constant values are limited to constant expressions",
                    self.file(),
                    node.location().clone(),
                );

                ConstExpression::Invalid(Box::new(node.location().clone()))
            }
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
                    format!("This Int literal is invalid: {}", e),
                    self.file(),
                    node.location.clone(),
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
                    format!("This Float literal is invalid: {}", e),
                    self.file(),
                    node.location.clone(),
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

    fn single_string_literal(
        &self,
        node: ast::StringLiteral,
    ) -> Box<StringLiteral> {
        let mut values = Vec::new();

        if let Some(value) = node.value {
            values.push(self.string_text(value))
        }

        Box::new(StringLiteral {
            values,
            resolved_type: types::TypeRef::Unknown,
            location: node.location,
        })
    }

    fn double_string_literal(
        &mut self,
        node: ast::DoubleStringLiteral,
    ) -> Box<StringLiteral> {
        let values = node
            .values
            .into_iter()
            .map(|n| match n {
                ast::DoubleStringValue::Text(node) => self.string_text(*node),
                ast::DoubleStringValue::Expression(node) => {
                    let rec = self.expression(node.value);
                    let loc = rec.location().clone();

                    StringValue::Expression(Box::new(Call {
                        kind: types::CallKind::Unknown,
                        receiver: Some(rec),
                        name: Identifier {
                            name: types::TO_STRING_METHOD.to_string(),
                            location: loc.clone(),
                        },
                        arguments: Vec::new(),
                        else_block: None,
                        location: loc,
                    }))
                }
            })
            .collect();

        Box::new(StringLiteral {
            values,
            resolved_type: types::TypeRef::Unknown,
            location: node.location,
        })
    }

    fn string_text(&self, node: ast::StringText) -> StringValue {
        StringValue::Text(Box::new(StringText {
            value: node.value,
            location: node.location,
        }))
    }

    fn array_literal(&mut self, node: ast::Array) -> Box<ArrayLiteral> {
        Box::new(ArrayLiteral {
            value_type: types::TypeRef::Unknown,
            resolved_type: types::TypeRef::Unknown,
            values: self.values(node.values),
            location: node.location,
        })
    }

    fn tuple_literal(&mut self, node: ast::Tuple) -> Box<TupleLiteral> {
        Box::new(TupleLiteral {
            class_id: None,
            value_types: Vec::new(),
            resolved_type: types::TypeRef::Unknown,
            values: self.values(node.values),
            location: node.location,
        })
    }

    fn const_single_string_literal(
        &self,
        node: ast::StringLiteral,
    ) -> Box<ConstStringLiteral> {
        let value =
            node.value.map(|n| n.value).unwrap_or_else(|| "".to_string());

        Box::new(ConstStringLiteral {
            value,
            resolved_type: types::TypeRef::Unknown,
            location: node.location,
        })
    }

    fn const_double_string_literal(
        &mut self,
        node: ast::DoubleStringLiteral,
    ) -> ConstExpression {
        let mut value = String::new();

        // While we could in theory support string interpolation, for the sake
        // of simplicity we don't. This ensures we don't have to main two
        // versions of string conversion for constant types: one in the standard
        // library, and one here in the compiler.
        for val in node.values {
            match val {
                ast::DoubleStringValue::Text(node) => value += &node.value,
                ast::DoubleStringValue::Expression(node) => {
                    self.state.diagnostics.error(
                        DiagnosticId::InvalidConstExpr,
                        "Constant values don't support string interpolation",
                        self.file(),
                        node.location.clone(),
                    );

                    return ConstExpression::Invalid(Box::new(node.location));
                }
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
        let operator = self.binary_operator(&node.operator);

        Box::new(ConstBinary { left, right, operator, resolved_type, location })
    }

    fn const_array(&mut self, node: ast::Array) -> Box<ConstArray> {
        let values =
            node.values.into_iter().map(|n| self.const_value(n)).collect();

        Box::new(ConstArray {
            resolved_type: types::TypeRef::Unknown,
            values,
            location: node.location,
        })
    }

    fn binary_operator(&self, operator: &ast::Operator) -> Operator {
        // This isn't ideal, but I also don't want to introduce a standalone
        // Operator enum in its own module _just_ so we don't need this match.
        match operator.kind {
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
        nodes.into_iter().map(|n| self.expression(n)).collect()
    }

    fn expression(&mut self, node: ast::Expression) -> Expression {
        match node {
            ast::Expression::Int(node) => {
                Expression::Int(Box::new(self.int_literal(*node)))
            }
            ast::Expression::SingleString(node) => {
                Expression::String(self.single_string_literal(*node))
            }
            ast::Expression::DoubleString(node) => {
                Expression::String(self.double_string_literal(*node))
            }
            ast::Expression::Float(node) => {
                Expression::Float(self.float_literal(*node))
            }
            ast::Expression::Binary(node) => {
                Expression::Call(self.binary(*node, None))
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
            ast::Expression::Index(node) => {
                Expression::Index(self.index_expression(*node))
            }
            ast::Expression::SetIndex(node) => {
                Expression::Call(self.set_index_expression(*node))
            }
            ast::Expression::Throw(node) => {
                Expression::Throw(self.throw_expression(*node))
            }
            ast::Expression::Return(node) => {
                Expression::Return(self.return_expression(*node))
            }
            ast::Expression::Try(node) => self.try_expression(*node),
            ast::Expression::TryPanic(node) => self.try_panic(*node),
            ast::Expression::If(node) => {
                Expression::Match(self.if_expression(*node))
            }
            ast::Expression::Loop(node) => {
                Expression::Loop(self.loop_expression(*node))
            }
            ast::Expression::While(node) => {
                Expression::Loop(self.while_expression(*node))
            }
            ast::Expression::Scope(node) => {
                Expression::Scope(self.scope(*node))
            }
            ast::Expression::Match(node) => {
                Expression::Match(self.match_expression(*node))
            }
            ast::Expression::ClassLiteral(node) => {
                Expression::ClassLiteral(self.instance_literal(*node))
            }
            ast::Expression::Array(node) => {
                Expression::Array(self.array_literal(*node))
            }
            ast::Expression::Tuple(node) => {
                Expression::Tuple(self.tuple_literal(*node))
            }
        }
    }

    fn binary(
        &mut self,
        node: ast::Binary,
        else_block: Option<ElseBlock>,
    ) -> Box<Call> {
        let op = self.binary_operator(&node.operator);

        Box::new(Call {
            kind: types::CallKind::Unknown,
            receiver: Some(self.expression(node.left)),
            name: Identifier {
                name: op.method_name().to_string(),
                location: node.operator.location,
            },
            arguments: vec![Argument::Positional(Box::new(
                self.expression(node.right),
            ))],
            else_block,
            location: node.location,
        })
    }

    fn field_ref(&self, node: ast::Field) -> Box<FieldRef> {
        Box::new(FieldRef {
            field_id: None,
            name: node.name,
            resolved_type: types::TypeRef::Unknown,
            location: node.location,
        })
    }

    fn constant_ref(&self, node: ast::Constant) -> Box<ConstantRef> {
        Box::new(ConstantRef {
            kind: types::ConstantKind::Unknown,
            source: self.optional_identifier(node.source),
            name: node.name,
            resolved_type: types::TypeRef::Unknown,
            location: node.location,
        })
    }

    fn identifier_ref(&self, node: ast::Identifier) -> Box<IdentifierRef> {
        Box::new(IdentifierRef {
            kind: types::IdentifierKind::Unknown,
            name: node.name,
            location: node.location,
        })
    }

    fn call(&mut self, node: ast::Call) -> Expression {
        if self.is_builtin_call(&node) {
            if !self.module.is_std(&self.state.db) {
                self.state.diagnostics.invalid_builtin_function(
                    self.file(),
                    node.location.clone(),
                );
            }

            return Expression::BuiltinCall(Box::new(BuiltinCall {
                info: None,
                name: self.identifier(node.name),
                arguments: self.optional_builtin_call_arguments(node.arguments),
                else_block: None,
                location: node.location,
            }));
        }

        Expression::Call(Box::new(Call {
            kind: types::CallKind::Unknown,
            receiver: node.receiver.map(|n| self.expression(n)),
            name: self.identifier(node.name),
            arguments: self.optional_call_arguments(node.arguments),
            else_block: None,
            location: node.location,
        }))
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
                            "Builtin calls don't support named arguments",
                            self.file(),
                            node.location,
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
                        Argument::Positional(Box::new(self.expression(node)))
                    }
                    ast::Argument::Named(node) => {
                        Argument::Named(Box::new(NamedArgument {
                            index: 0,
                            name: self.identifier(node.name),
                            value: self.expression(node.value),
                            location: node.location,
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
        let op = self.binary_operator(&node.operator);
        let variable = self.identifier(node.variable);
        let receiver = Expression::IdentifierRef(Box::new(IdentifierRef {
            kind: types::IdentifierKind::Unknown,
            name: variable.name.clone(),
            location: variable.location.clone(),
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
                arguments: vec![Argument::Positional(Box::new(
                    self.expression(node.value),
                ))],
                else_block: None,
                location: node.location.clone(),
            })),
            resolved_type: types::TypeRef::Unknown,
            location: node.location,
        })
    }

    fn binary_assign_field(
        &mut self,
        node: ast::BinaryAssignField,
    ) -> Box<AssignField> {
        let op = self.binary_operator(&node.operator);
        let field = self.field(node.field);
        let receiver = Expression::FieldRef(Box::new(FieldRef {
            field_id: None,
            name: field.name.clone(),
            resolved_type: types::TypeRef::Unknown,
            location: field.location.clone(),
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
                arguments: vec![Argument::Positional(Box::new(
                    self.expression(node.value),
                ))],
                else_block: None,
                location: node.location.clone(),
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
            else_block: None,
            location: node.location,
        })
    }

    fn binary_assign_setter(
        &mut self,
        node: ast::BinaryAssignSetter,
    ) -> Box<AssignSetter> {
        let op = self.binary_operator(&node.operator);
        let name = self.identifier(node.name);
        let setter_rec = self.expression(node.receiver);
        let getter_loc =
            SourceLocation::start_end(setter_rec.location(), &name.location);
        let getter_rec = Expression::Call(Box::new(Call {
            kind: types::CallKind::Unknown,
            receiver: Some(setter_rec.clone()),
            name: name.clone(),
            arguments: Vec::new(),
            else_block: None,
            location: getter_loc,
        }));

        Box::new(AssignSetter {
            kind: types::CallKind::Unknown,
            receiver: setter_rec,
            name,
            value: Expression::Call(Box::new(Call {
                kind: types::CallKind::Unknown,
                receiver: Some(getter_rec),
                arguments: vec![Argument::Positional(Box::new(
                    self.expression(node.value),
                ))],
                name: Identifier {
                    name: op.method_name().to_string(),
                    location: node.operator.location,
                },
                else_block: None,
                location: node.location.clone(),
            })),
            else_block: None,
            location: node.location,
        })
    }

    fn closure(&mut self, node: ast::Closure) -> Box<Closure> {
        Box::new(Closure {
            closure_id: None,
            resolved_type: types::TypeRef::Unknown,
            moving: node.moving,
            arguments: self.optional_block_arguments(node.arguments),
            throw_type: node.throw_type.map(|n| self.type_reference(n)),
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
        Box::new(Mut {
            resolved_type: types::TypeRef::Unknown,
            value: self.expression(node.value),
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

    fn index_expression(&mut self, node: ast::Index) -> Box<Index> {
        Box::new(Index {
            info: None,
            receiver: self.expression(node.receiver),
            index: self.expression(node.index),
            location: node.location,
        })
    }

    fn set_index_expression(&mut self, node: ast::SetIndex) -> Box<Call> {
        Box::new(Call {
            kind: types::CallKind::Unknown,
            receiver: Some(self.expression(node.receiver)),
            name: Identifier {
                name: SET_INDEX_METHOD.to_string(),
                location: node.location.clone(),
            },
            arguments: vec![
                Argument::Positional(Box::new(self.expression(node.index))),
                Argument::Positional(Box::new(self.expression(node.value))),
            ],
            else_block: None,
            location: node.location,
        })
    }

    fn throw_expression(&mut self, node: ast::Throw) -> Box<Throw> {
        Box::new(Throw {
            resolved_type: types::TypeRef::Unknown,
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

    /// Desugars a `try` expression such that it always has an explicit `else`.
    ///
    /// This desugars this:
    ///
    ///     try x
    ///
    /// Into this:
    ///
    ///     try x else (error) throw error
    ///
    /// If an explicit `else` is already present, no desugaring is applied.
    fn try_expression(&mut self, node: ast::Try) -> Expression {
        let else_block = if let Some(else_block) = node.else_block {
            let body = self.expressions(else_block.body);
            let binding = else_block.argument.map(|n| self.block_argument(n));

            ElseBlock { body, argument: binding, location: else_block.location }
        } else {
            let location = node.location.clone();
            let throw = self.throw_variable(TRY_BINDING_VAR, location.clone());
            let binding = self.generated_variable_definition(
                TRY_BINDING_VAR,
                location.clone(),
            );

            ElseBlock { body: vec![throw], argument: Some(binding), location }
        };

        self.try_else(node.try_block, else_block, node.location)
    }

    /// Desugars a `try!` expression into a `try` that panics.
    ///
    /// Expressions like this:
    ///
    ///     try! x
    ///
    /// Are desugared into this:
    ///
    ///     try x else (error) _INKO.panic(error.to_string)
    fn try_panic(&mut self, node: ast::TryPanic) -> Expression {
        let location = node.location.clone();
        let panic = self.hidden_panic(TRY_BINDING_VAR, location.clone());
        let binding = self
            .generated_variable_definition(TRY_BINDING_VAR, location.clone());
        let else_block =
            ElseBlock { body: vec![panic], argument: Some(binding), location };

        self.try_else(node.try_block, else_block, node.location)
    }

    fn try_else(
        &mut self,
        try_block: ast::TryBlock,
        else_block: ElseBlock,
        location: SourceLocation,
    ) -> Expression {
        match try_block.value {
            ast::Expression::Identifier(n) => {
                Expression::Call(Box::new(Call {
                    kind: types::CallKind::Unknown,
                    receiver: None,
                    name: self.identifier(*n),
                    arguments: Vec::new(),
                    else_block: Some(else_block),
                    location,
                }))
            }
            ast::Expression::Constant(n) => Expression::Call(Box::new(Call {
                kind: types::CallKind::Unknown,
                receiver: None,
                name: Identifier { name: n.name, location: n.location },
                arguments: Vec::new(),
                else_block: Some(else_block),
                location,
            })),
            ast::Expression::Call(call) => {
                if self.is_builtin_call(&call) {
                    if !self.module.is_std(&self.state.db) {
                        self.state.diagnostics.invalid_builtin_function(
                            self.file(),
                            call.location.clone(),
                        );
                    }

                    Expression::BuiltinCall(Box::new(BuiltinCall {
                        info: None,
                        name: self.identifier(call.name),
                        arguments: self
                            .optional_builtin_call_arguments(call.arguments),
                        else_block: Some(else_block),
                        location,
                    }))
                } else {
                    Expression::Call(Box::new(Call {
                        kind: types::CallKind::Unknown,
                        receiver: call.receiver.map(|n| self.expression(n)),
                        name: self.identifier(call.name),
                        arguments: self.optional_call_arguments(call.arguments),
                        else_block: Some(else_block),
                        location,
                    }))
                }
            }
            ast::Expression::AssignSetter(node) => {
                Expression::AssignSetter(Box::new(AssignSetter {
                    kind: types::CallKind::Unknown,
                    receiver: self.expression(node.receiver),
                    name: self.identifier(node.name),
                    value: self.expression(node.value),
                    else_block: Some(else_block),
                    location,
                }))
            }
            ast::Expression::Binary(node) => {
                Expression::Call(self.binary(*node, Some(else_block)))
            }
            _ => {
                let loc = try_block.value.location().clone();

                self.state.diagnostics.never_throws(self.file(), loc.clone());
                Expression::Invalid(Box::new(loc))
            }
        }
    }

    fn if_expression(&mut self, node: ast::If) -> Box<Match> {
        let mut cases = vec![MatchCase {
            variable_ids: Vec::new(),
            pattern: Pattern::True(Box::new(True {
                resolved_type: types::TypeRef::Unknown,
                location: node.if_true.condition.location().clone(),
            })),
            guard: None,
            body: self.expressions(node.if_true.body),
            location: node.if_true.location,
        }];

        for cond in node.else_if {
            cases.push(MatchCase {
                variable_ids: Vec::new(),
                pattern: Pattern::Wildcard(Box::new(WildcardPattern {
                    location: cond.location.clone(),
                })),
                guard: Some(self.expression(cond.condition)),
                body: self.expressions(cond.body),
                location: cond.location,
            });
        }

        let mut has_else = false;

        if let Some(body) = node.else_body {
            let location = body.location.clone();

            has_else = true;

            cases.push(MatchCase {
                variable_ids: Vec::new(),
                pattern: Pattern::Wildcard(Box::new(WildcardPattern {
                    location: body.location.clone(),
                })),
                guard: None,
                body: self.expressions(body),
                location,
            })
        } else {
            cases.push(MatchCase {
                variable_ids: Vec::new(),
                pattern: Pattern::Wildcard(Box::new(WildcardPattern {
                    location: node.location.clone(),
                })),
                guard: None,
                body: vec![Expression::Nil(Box::new(Nil {
                    resolved_type: types::TypeRef::Unknown,
                    location: node.location.clone(),
                }))],
                location: node.location.clone(),
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
        let location = node.condition.location().clone();
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
                        location: location.clone(),
                    })),
                    guard: None,
                    body: cond_body,
                    location: location.clone(),
                },
                MatchCase {
                    variable_ids: Vec::new(),
                    pattern: Pattern::Wildcard(Box::new(WildcardPattern {
                        location: location.clone(),
                    })),
                    guard: None,
                    body: vec![self.break_expression(location)],
                    location: node.location.clone(),
                },
            ],
            location: node.location.clone(),
            write_result: true,
        }))];

        Box::new(Loop { body, location: node.location })
    }

    fn scope(&mut self, node: ast::Scope) -> Box<Scope> {
        Box::new(Scope {
            resolved_type: types::TypeRef::Unknown,
            body: self.expressions(node.body),
            location: node.location,
        })
    }

    fn instance_literal(
        &mut self,
        node: ast::ClassLiteral,
    ) -> Box<ClassLiteral> {
        Box::new(ClassLiteral {
            class_id: None,
            resolved_type: types::TypeRef::Unknown,
            class_name: self.constant(node.class_name),
            fields: node
                .fields
                .into_iter()
                .map(|n| AssignInstanceLiteralField {
                    resolved_type: types::TypeRef::Unknown,
                    field_id: None,
                    field: self.field(n.field),
                    value: self.expression(n.value),
                    location: n.location,
                })
                .collect(),
            location: node.location,
        })
    }

    fn match_expression(&mut self, node: ast::Match) -> Box<Match> {
        Box::new(Match {
            resolved_type: types::TypeRef::Unknown,
            expression: self.expression(node.expression),
            cases: node
                .cases
                .into_iter()
                .map(|node| MatchCase {
                    variable_ids: Vec::new(),
                    pattern: self.pattern(node.pattern),
                    guard: node.guard.map(|n| self.expression(n)),
                    body: self.expressions(node.body),
                    location: node.location,
                })
                .collect(),
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
            ast::Pattern::Expression(n) => match *n {
                ast::Expression::Int(n) => {
                    Pattern::Int(Box::new(self.int_literal(*n)))
                }
                ast::Expression::True(n) => {
                    Pattern::True(self.true_literal(*n))
                }
                ast::Expression::False(n) => {
                    Pattern::False(self.false_literal(*n))
                }
                _ => {
                    unreachable!("This pattern isn't supported")
                }
            },
            ast::Pattern::Variant(n) => {
                Pattern::Variant(Box::new(VariantPattern {
                    variant_id: None,
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
            ast::Pattern::Class(n) => Pattern::Class(Box::new(ClassPattern {
                class_id: None,
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
                Pattern::String(Box::new(StringPattern {
                    value: n.value,
                    location: n.location,
                }))
            }
        }
    }

    fn patterns(&mut self, nodes: Vec<ast::Pattern>) -> Vec<Pattern> {
        nodes.into_iter().map(|n| self.pattern(n)).collect()
    }

    fn throw_variable(
        &self,
        name: &str,
        location: SourceLocation,
    ) -> Expression {
        Expression::Throw(Box::new(Throw {
            resolved_type: types::TypeRef::Unknown,
            value: Expression::IdentifierRef(Box::new(IdentifierRef {
                kind: types::IdentifierKind::Unknown,
                name: name.to_string(),
                location: location.clone(),
            })),
            location,
        }))
    }

    fn hidden_panic(
        &self,
        variable: &str,
        location: SourceLocation,
    ) -> Expression {
        Expression::BuiltinCall(Box::new(BuiltinCall {
            info: None,
            name: Identifier {
                name: types::BuiltinFunction::PanicThrown.name().to_string(),
                location: location.clone(),
            },
            arguments: vec![Expression::IdentifierRef(Box::new(
                IdentifierRef {
                    kind: types::IdentifierKind::Unknown,
                    name: variable.to_string(),
                    location: location.clone(),
                },
            ))],
            else_block: None,
            location,
        }))
    }

    fn generated_variable_definition(
        &self,
        name: &str,
        location: SourceLocation,
    ) -> BlockArgument {
        BlockArgument {
            variable_id: None,
            name: Identifier {
                name: name.to_string(),
                location: location.clone(),
            },
            value_type: None,
            location,
        }
    }

    fn break_expression(&self, location: SourceLocation) -> Expression {
        Expression::Break(Box::new(Break { location }))
    }

    fn operator_method_not_allowed(
        &mut self,
        operator: bool,
        location: &SourceLocation,
    ) {
        if !operator {
            return;
        }

        self.state.diagnostics.error(
            DiagnosticId::InvalidMethod,
            "Operator methods must be regular instance methods",
            self.file(),
            location.clone(),
        );
    }

    fn check_operator_requirements(&mut self, node: &ast::DefineMethod) {
        if !node.operator {
            return;
        }

        if let Some(throws) = node.throw_type.as_ref() {
            self.state.diagnostics.error(
                DiagnosticId::InvalidMethod,
                "Operator methods can't throw",
                self.file(),
                throws.location().clone(),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::test::cols;
    use ::ast::parser::Parser;
    use similar_asserts::assert_eq;

    use types::module_name::ModuleName;

    fn parse(input: &str) -> ParsedModule {
        let name = ModuleName::new("std::foo");
        let ast = Parser::new(input.into(), "test.inko".into())
            .parse()
            .expect("Failed to parse the module");

        ParsedModule { ast, name }
    }

    fn lower(input: &str) -> (Module, usize) {
        let mut state = State::new(Config::new());
        let ast = parse(input);
        let mut hir = LowerToHir::run_all(&mut state, vec![ast]);

        (hir.pop().unwrap(), state.diagnostics.iter().count())
    }

    fn lower_top_expr(input: &str) -> (TopLevelExpression, usize) {
        let (mut module, diags) = lower(input);

        (module.expressions.pop().unwrap(), diags)
    }

    fn lower_type(input: &str) -> Type {
        let hir =
            lower(&format!("fn a(a: {}) {{}}", input)).0.expressions.remove(0);

        match hir {
            TopLevelExpression::ModuleMethod(mut node) => {
                node.arguments.remove(0).value_type
            }
            _ => {
                panic!("The top-level expression must be a module method")
            }
        }
    }

    fn lower_expr(input: &str) -> (Expression, usize) {
        let (mut top, diags) = lower(input);
        let hir = top.expressions.remove(0);

        match hir {
            TopLevelExpression::ModuleMethod(mut node) => {
                (node.body.remove(0), diags)
            }
            _ => {
                panic!("The top-level expression must be a module method")
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
    fn test_lower_constant_with_string_interpolation() {
        let (hir, diags) = lower_top_expr("let A = \"{10}\"");

        assert_eq!(diags, 1);
        assert_eq!(
            hir,
            TopLevelExpression::Constant(Box::new(DefineConstant {
                public: false,
                constant_id: None,
                name: Constant { name: "A".to_string(), location: cols(5, 5) },
                value: ConstExpression::Invalid(Box::new(cols(10, 13))),
                location: cols(1, 14)
            }))
        );
    }

    #[test]
    fn test_lower_constant_with_int() {
        let (hir, diags) = lower_top_expr("let A = 10");

        assert_eq!(diags, 0);
        assert_eq!(
            hir,
            TopLevelExpression::Constant(Box::new(DefineConstant {
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
                    location: cols(11, 11)
                }))],
                location: cols(9, 12)
            }))
        );
    }

    #[test]
    fn test_lower_namespaced_type_name() {
        let hir = lower_type("a::B");

        assert_eq!(
            hir,
            Type::Named(Box::new(TypeName {
                source: Some(Identifier {
                    name: "a".to_string(),
                    location: cols(9, 9)
                }),
                resolved_type: types::TypeRef::Unknown,
                name: Constant {
                    name: "B".to_string(),
                    location: cols(12, 12)
                },
                arguments: Vec::new(),
                location: cols(9, 12)
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
    fn test_lower_closure_type() {
        let hir = lower_type("fn (A) !! B -> C");

        assert_eq!(
            hir,
            Type::Closure(Box::new(ClosureType {
                arguments: vec![Type::Named(Box::new(TypeName {
                    source: None,
                    resolved_type: types::TypeRef::Unknown,
                    name: Constant {
                        name: "A".to_string(),
                        location: cols(13, 13)
                    },
                    arguments: Vec::new(),
                    location: cols(13, 13)
                }))],
                throw_type: Some(Type::Named(Box::new(TypeName {
                    source: None,
                    resolved_type: types::TypeRef::Unknown,
                    name: Constant {
                        name: "B".to_string(),
                        location: cols(19, 19)
                    },
                    arguments: Vec::new(),
                    location: cols(19, 19)
                }))),
                return_type: Some(Type::Named(Box::new(TypeName {
                    source: None,
                    resolved_type: types::TypeRef::Unknown,
                    name: Constant {
                        name: "C".to_string(),
                        location: cols(24, 24)
                    },
                    arguments: Vec::new(),
                    location: cols(24, 24)
                }))),
                location: cols(9, 24),
                resolved_type: types::TypeRef::Unknown,
            }))
        );
    }

    #[test]
    fn test_lower_module_method() {
        let (hir, diags) =
            lower_top_expr("fn foo[A: X](a: B) !! C -> D { 10 }");

        assert_eq!(diags, 0);
        assert_eq!(
            hir,
            TopLevelExpression::ModuleMethod(Box::new(DefineModuleMethod {
                public: false,
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
                        source: None,
                        resolved_type: types::TypeRef::Unknown,
                        name: Constant {
                            name: "X".to_string(),
                            location: cols(11, 11)
                        },
                        arguments: Vec::new(),
                        location: cols(11, 11)
                    }],
                    location: cols(8, 11)
                }],
                arguments: vec![MethodArgument {
                    name: Identifier {
                        name: "a".to_string(),
                        location: cols(14, 14)
                    },
                    value_type: Type::Named(Box::new(TypeName {
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
                throw_type: Some(Type::Named(Box::new(TypeName {
                    source: None,
                    resolved_type: types::TypeRef::Unknown,
                    name: Constant {
                        name: "C".to_string(),
                        location: cols(23, 23)
                    },
                    arguments: Vec::new(),
                    location: cols(23, 23)
                }))),
                return_type: Some(Type::Named(Box::new(TypeName {
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
                location: cols(1, 35),
            })),
        );
    }

    #[test]
    fn test_lower_class() {
        let hir = lower_top_expr("class A[B: C] { let @a: B }").0;

        assert_eq!(
            hir,
            TopLevelExpression::Class(Box::new(DefineClass {
                public: false,
                kind: ClassKind::Regular,
                class_id: None,
                name: Constant { name: "A".to_string(), location: cols(7, 7) },
                type_parameters: vec![TypeParameter {
                    type_parameter_id: None,
                    name: Constant {
                        name: "B".to_string(),
                        location: cols(9, 9)
                    },
                    requirements: vec![TypeName {
                        source: None,
                        resolved_type: types::TypeRef::Unknown,
                        name: Constant {
                            name: "C".to_string(),
                            location: cols(12, 12)
                        },
                        arguments: Vec::new(),
                        location: cols(12, 12)
                    }],
                    location: cols(9, 12)
                }],
                body: vec![ClassExpression::Field(Box::new(DefineField {
                    public: false,
                    field_id: None,
                    name: Identifier {
                        name: "a".to_string(),
                        location: cols(21, 22)
                    },
                    value_type: Type::Named(Box::new(TypeName {
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
                }))],
                location: cols(1, 27)
            })),
        );
    }

    #[test]
    fn test_lower_public_class() {
        let hir = lower_top_expr("class pub A {}").0;

        assert_eq!(
            hir,
            TopLevelExpression::Class(Box::new(DefineClass {
                public: true,
                kind: ClassKind::Regular,
                class_id: None,
                name: Constant {
                    name: "A".to_string(),
                    location: cols(11, 11)
                },
                type_parameters: Vec::new(),
                body: Vec::new(),
                location: cols(1, 14)
            })),
        );
    }

    #[test]
    fn test_lower_class_with_public_field() {
        let hir = lower_top_expr("class A { let pub @a: A }").0;

        assert_eq!(
            hir,
            TopLevelExpression::Class(Box::new(DefineClass {
                public: false,
                kind: ClassKind::Regular,
                class_id: None,
                name: Constant { name: "A".to_string(), location: cols(7, 7) },
                type_parameters: Vec::new(),
                body: vec![ClassExpression::Field(Box::new(DefineField {
                    public: true,
                    field_id: None,
                    name: Identifier {
                        name: "a".to_string(),
                        location: cols(19, 20)
                    },
                    value_type: Type::Named(Box::new(TypeName {
                        source: None,
                        resolved_type: types::TypeRef::Unknown,
                        name: Constant {
                            name: "A".to_string(),
                            location: cols(23, 23)
                        },
                        arguments: Vec::new(),
                        location: cols(23, 23)
                    })),
                    location: cols(11, 23)
                }))],
                location: cols(1, 25)
            })),
        );
    }

    #[test]
    fn test_lower_builtin_class() {
        let hir = lower_top_expr("class builtin A[B: C] { let @a: B }").0;

        assert_eq!(
            hir,
            TopLevelExpression::Class(Box::new(DefineClass {
                public: false,
                class_id: None,
                kind: ClassKind::Builtin,
                name: Constant {
                    name: "A".to_string(),
                    location: cols(15, 15)
                },
                type_parameters: vec![TypeParameter {
                    type_parameter_id: None,
                    name: Constant {
                        name: "B".to_string(),
                        location: cols(17, 17)
                    },
                    requirements: vec![TypeName {
                        source: None,
                        resolved_type: types::TypeRef::Unknown,
                        name: Constant {
                            name: "C".to_string(),
                            location: cols(20, 20)
                        },
                        arguments: Vec::new(),
                        location: cols(20, 20)
                    }],
                    location: cols(17, 20)
                }],
                body: vec![ClassExpression::Field(Box::new(DefineField {
                    public: false,
                    field_id: None,
                    name: Identifier {
                        name: "a".to_string(),
                        location: cols(29, 30)
                    },
                    value_type: Type::Named(Box::new(TypeName {
                        source: None,
                        resolved_type: types::TypeRef::Unknown,
                        name: Constant {
                            name: "B".to_string(),
                            location: cols(33, 33)
                        },
                        arguments: Vec::new(),
                        location: cols(33, 33)
                    })),
                    location: cols(25, 33),
                }))],
                location: cols(1, 35)
            })),
        );
    }

    #[test]
    fn test_lower_async_class() {
        let hir = lower_top_expr("class async A {}").0;

        assert_eq!(
            hir,
            TopLevelExpression::Class(Box::new(DefineClass {
                public: false,
                class_id: None,
                kind: ClassKind::Async,
                name: Constant {
                    name: "A".to_string(),
                    location: cols(13, 13)
                },
                type_parameters: Vec::new(),
                body: Vec::new(),
                location: cols(1, 16)
            })),
        );
    }

    #[test]
    fn test_lower_class_with_static_method() {
        let hir =
            lower_top_expr("class A { fn static a[A](b: B) !! C -> D { 10 } }")
                .0;

        assert_eq!(
            hir,
            TopLevelExpression::Class(Box::new(DefineClass {
                public: false,
                class_id: None,
                kind: ClassKind::Regular,
                name: Constant { name: "A".to_string(), location: cols(7, 7) },
                type_parameters: Vec::new(),
                body: vec![ClassExpression::StaticMethod(Box::new(
                    DefineStaticMethod {
                        public: false,
                        name: Identifier {
                            name: "a".to_string(),
                            location: cols(21, 21)
                        },
                        type_parameters: vec![TypeParameter {
                            type_parameter_id: None,
                            name: Constant {
                                name: "A".to_string(),
                                location: cols(23, 23)
                            },
                            requirements: Vec::new(),
                            location: cols(23, 23)
                        }],
                        arguments: vec![MethodArgument {
                            name: Identifier {
                                name: "b".to_string(),
                                location: cols(26, 26)
                            },
                            value_type: Type::Named(Box::new(TypeName {
                                source: None,
                                resolved_type: types::TypeRef::Unknown,
                                name: Constant {
                                    name: "B".to_string(),
                                    location: cols(29, 29)
                                },
                                arguments: Vec::new(),
                                location: cols(29, 29)
                            })),
                            location: cols(26, 29)
                        }],
                        throw_type: Some(Type::Named(Box::new(TypeName {
                            source: None,
                            resolved_type: types::TypeRef::Unknown,
                            name: Constant {
                                name: "C".to_string(),
                                location: cols(35, 35)
                            },
                            arguments: Vec::new(),
                            location: cols(35, 35)
                        }))),
                        return_type: Some(Type::Named(Box::new(TypeName {
                            source: None,
                            resolved_type: types::TypeRef::Unknown,
                            name: Constant {
                                name: "D".to_string(),
                                location: cols(40, 40)
                            },
                            arguments: Vec::new(),
                            location: cols(40, 40)
                        }))),
                        body: vec![Expression::Int(Box::new(IntLiteral {
                            value: 10,
                            resolved_type: types::TypeRef::Unknown,
                            location: cols(44, 45)
                        }))],
                        method_id: None,
                        location: cols(11, 47),
                    }
                ))],
                location: cols(1, 49)
            })),
        );
    }

    #[test]
    fn test_lower_class_with_async_method() {
        let hir =
            lower_top_expr("class A { fn async a[A](b: B) !! C -> D { 10 } }")
                .0;

        assert_eq!(
            hir,
            TopLevelExpression::Class(Box::new(DefineClass {
                public: false,
                class_id: None,
                kind: ClassKind::Regular,
                name: Constant { name: "A".to_string(), location: cols(7, 7) },
                type_parameters: Vec::new(),
                body: vec![ClassExpression::AsyncMethod(Box::new(
                    DefineAsyncMethod {
                        mutable: false,
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
                            location: cols(22, 22)
                        }],
                        arguments: vec![MethodArgument {
                            name: Identifier {
                                name: "b".to_string(),
                                location: cols(25, 25)
                            },
                            value_type: Type::Named(Box::new(TypeName {
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
                        throw_type: Some(Type::Named(Box::new(TypeName {
                            source: None,
                            resolved_type: types::TypeRef::Unknown,
                            name: Constant {
                                name: "C".to_string(),
                                location: cols(34, 34)
                            },
                            arguments: Vec::new(),
                            location: cols(34, 34)
                        }))),
                        return_type: Some(Type::Named(Box::new(TypeName {
                            source: None,
                            resolved_type: types::TypeRef::Unknown,
                            name: Constant {
                                name: "D".to_string(),
                                location: cols(39, 39)
                            },
                            arguments: Vec::new(),
                            location: cols(39, 39)
                        }))),
                        body: vec![Expression::Int(Box::new(IntLiteral {
                            value: 10,
                            resolved_type: types::TypeRef::Unknown,
                            location: cols(43, 44)
                        }))],
                        method_id: None,
                        location: cols(11, 46),
                    }
                ))],
                location: cols(1, 48)
            })),
        );
    }

    #[test]
    fn test_lower_class_with_instance_method() {
        let hir =
            lower_top_expr("class A { fn a[A](b: B) !! C -> D { 10 } }").0;

        assert_eq!(
            hir,
            TopLevelExpression::Class(Box::new(DefineClass {
                public: false,
                class_id: None,
                kind: ClassKind::Regular,
                name: Constant { name: "A".to_string(), location: cols(7, 7) },
                type_parameters: Vec::new(),
                body: vec![ClassExpression::InstanceMethod(Box::new(
                    DefineInstanceMethod {
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
                            location: cols(16, 16)
                        }],
                        arguments: vec![MethodArgument {
                            name: Identifier {
                                name: "b".to_string(),
                                location: cols(19, 19)
                            },
                            value_type: Type::Named(Box::new(TypeName {
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
                        throw_type: Some(Type::Named(Box::new(TypeName {
                            source: None,
                            resolved_type: types::TypeRef::Unknown,
                            name: Constant {
                                name: "C".to_string(),
                                location: cols(28, 28)
                            },
                            arguments: Vec::new(),
                            location: cols(28, 28)
                        }))),
                        return_type: Some(Type::Named(Box::new(TypeName {
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
                        location: cols(11, 40)
                    }
                ))],
                location: cols(1, 42)
            })),
        );
    }

    #[test]
    fn test_lower_instance_operator_method_with_throw() {
        let diags = lower_top_expr("class A { fn + !! A {} }").1;

        assert_eq!(diags, 1);
    }

    #[test]
    fn test_lower_static_operator_method() {
        let diags = lower_top_expr("class A { fn static + {} }").1;

        assert_eq!(diags, 1);
    }

    #[test]
    fn test_lower_module_operator_method() {
        let diags = lower_top_expr("class A { fn static + {} }").1;

        assert_eq!(diags, 1);
    }

    #[test]
    fn test_lower_trait() {
        let hir = lower_top_expr("trait A[T]: B {}").0;

        assert_eq!(
            hir,
            TopLevelExpression::Trait(Box::new(DefineTrait {
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
                    location: cols(9, 9)
                }],
                requirements: vec![TypeName {
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
        let hir = lower_top_expr("trait A { fn a[A](b: B) !! C -> D }").0;

        assert_eq!(
            hir,
            TopLevelExpression::Trait(Box::new(DefineTrait {
                public: false,
                trait_id: None,
                name: Constant { name: "A".to_string(), location: cols(7, 7) },
                requirements: Vec::new(),
                type_parameters: Vec::new(),
                body: vec![TraitExpression::RequiredMethod(Box::new(
                    DefineRequiredMethod {
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
                            location: cols(16, 16)
                        }],
                        arguments: vec![MethodArgument {
                            name: Identifier {
                                name: "b".to_string(),
                                location: cols(19, 19)
                            },
                            value_type: Type::Named(Box::new(TypeName {
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
                        throw_type: Some(Type::Named(Box::new(TypeName {
                            source: None,
                            resolved_type: types::TypeRef::Unknown,
                            name: Constant {
                                name: "C".to_string(),
                                location: cols(28, 28)
                            },
                            arguments: Vec::new(),
                            location: cols(28, 28)
                        }))),
                        return_type: Some(Type::Named(Box::new(TypeName {
                            source: None,
                            resolved_type: types::TypeRef::Unknown,
                            name: Constant {
                                name: "D".to_string(),
                                location: cols(33, 33)
                            },
                            arguments: Vec::new(),
                            location: cols(33, 33)
                        }))),
                        method_id: None,
                        location: cols(11, 33)
                    }
                ))],
                location: cols(1, 35)
            }))
        );
    }

    #[test]
    fn test_lower_trait_with_moving_required_method() {
        let hir = lower_top_expr("trait A { fn move a }").0;

        assert_eq!(
            hir,
            TopLevelExpression::Trait(Box::new(DefineTrait {
                public: false,
                trait_id: None,
                name: Constant { name: "A".to_string(), location: cols(7, 7) },
                requirements: Vec::new(),
                type_parameters: Vec::new(),
                body: vec![TraitExpression::RequiredMethod(Box::new(
                    DefineRequiredMethod {
                        public: false,
                        kind: MethodKind::Moving,
                        name: Identifier {
                            name: "a".to_string(),
                            location: cols(19, 19)
                        },
                        type_parameters: Vec::new(),
                        arguments: Vec::new(),
                        throw_type: None,
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
                public: false,
                trait_id: None,
                name: Constant { name: "A".to_string(), location: cols(7, 7) },
                requirements: Vec::new(),
                type_parameters: Vec::new(),
                body: vec![TraitExpression::InstanceMethod(Box::new(
                    DefineInstanceMethod {
                        public: false,
                        kind: MethodKind::Moving,
                        name: Identifier {
                            name: "a".to_string(),
                            location: cols(19, 19)
                        },
                        type_parameters: Vec::new(),
                        arguments: Vec::new(),
                        throw_type: None,
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
        let hir =
            lower_top_expr("trait A { fn a[A](b: B) !! C -> D { 10 } }").0;

        assert_eq!(
            hir,
            TopLevelExpression::Trait(Box::new(DefineTrait {
                public: false,
                trait_id: None,
                name: Constant { name: "A".to_string(), location: cols(7, 7) },
                requirements: Vec::new(),
                type_parameters: Vec::new(),
                body: vec![TraitExpression::InstanceMethod(Box::new(
                    DefineInstanceMethod {
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
                            location: cols(16, 16)
                        }],
                        arguments: vec![MethodArgument {
                            name: Identifier {
                                name: "b".to_string(),
                                location: cols(19, 19)
                            },
                            value_type: Type::Named(Box::new(TypeName {
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
                        throw_type: Some(Type::Named(Box::new(TypeName {
                            source: None,
                            resolved_type: types::TypeRef::Unknown,
                            name: Constant {
                                name: "C".to_string(),
                                location: cols(28, 28)
                            },
                            arguments: Vec::new(),
                            location: cols(28, 28)
                        }))),
                        return_type: Some(Type::Named(Box::new(TypeName {
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
                        location: cols(11, 40)
                    }
                ))],
                location: cols(1, 42)
            }))
        );
    }

    #[test]
    fn test_lower_reopen_empty_class() {
        assert_eq!(
            lower_top_expr("impl A {}").0,
            TopLevelExpression::Reopen(Box::new(ReopenClass {
                class_id: None,
                class_name: Constant {
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
            TopLevelExpression::Reopen(Box::new(ReopenClass {
                class_id: None,
                class_name: Constant {
                    name: "A".to_string(),
                    location: cols(6, 6)
                },
                bounds: vec![TypeBound {
                    name: Constant {
                        name: "T".to_string(),
                        location: cols(11, 11)
                    },
                    requirements: vec![Requirement::Mutable(cols(14, 16))],
                    location: cols(11, 16),
                }],
                body: Vec::new(),
                location: cols(1, 16)
            }))
        );
    }

    #[test]
    fn test_lower_reopen_class_with_instance_method() {
        let hir = lower_top_expr("impl A { fn foo {} }").0;

        assert_eq!(
            hir,
            TopLevelExpression::Reopen(Box::new(ReopenClass {
                class_id: None,
                class_name: Constant {
                    name: "A".to_string(),
                    location: cols(6, 6)
                },
                body: vec![ReopenClassExpression::InstanceMethod(Box::new(
                    DefineInstanceMethod {
                        public: false,
                        kind: MethodKind::Regular,
                        name: Identifier {
                            name: "foo".to_string(),
                            location: cols(13, 15)
                        },
                        type_parameters: Vec::new(),
                        arguments: Vec::new(),
                        throw_type: None,
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
    fn test_lower_reopen_class_with_static_method() {
        let hir = lower_top_expr("impl A { fn static foo {} }").0;

        assert_eq!(
            hir,
            TopLevelExpression::Reopen(Box::new(ReopenClass {
                class_id: None,
                class_name: Constant {
                    name: "A".to_string(),
                    location: cols(6, 6)
                },
                body: vec![ReopenClassExpression::StaticMethod(Box::new(
                    DefineStaticMethod {
                        public: false,
                        name: Identifier {
                            name: "foo".to_string(),
                            location: cols(20, 22)
                        },
                        type_parameters: Vec::new(),
                        arguments: Vec::new(),
                        throw_type: None,
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
    fn test_lower_reopen_class_with_async_method() {
        let hir = lower_top_expr("impl A { fn async foo {} }").0;

        assert_eq!(
            hir,
            TopLevelExpression::Reopen(Box::new(ReopenClass {
                class_id: None,
                class_name: Constant {
                    name: "A".to_string(),
                    location: cols(6, 6)
                },
                body: vec![ReopenClassExpression::AsyncMethod(Box::new(
                    DefineAsyncMethod {
                        mutable: false,
                        public: false,
                        name: Identifier {
                            name: "foo".to_string(),
                            location: cols(19, 21)
                        },
                        type_parameters: Vec::new(),
                        arguments: Vec::new(),
                        throw_type: None,
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
    fn test_lower_empty_trait_implementation() {
        let hir = lower_top_expr("impl A for B {}").0;

        assert_eq!(
            hir,
            TopLevelExpression::Implement(Box::new(ImplementTrait {
                trait_name: TypeName {
                    source: None,
                    resolved_type: types::TypeRef::Unknown,
                    name: Constant {
                        name: "A".to_string(),
                        location: cols(6, 6)
                    },
                    arguments: Vec::new(),
                    location: cols(6, 6)
                },
                class_name: Constant {
                    name: "B".to_string(),
                    location: cols(12, 12)
                },
                bounds: Vec::new(),
                body: Vec::new(),
                location: cols(1, 15),
                trait_instance: None,
                class_instance: None,
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
                    source: None,
                    resolved_type: types::TypeRef::Unknown,
                    name: Constant {
                        name: "A".to_string(),
                        location: cols(6, 6)
                    },
                    arguments: vec![Type::Named(Box::new(TypeName {
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
                class_name: Constant {
                    name: "B".to_string(),
                    location: cols(15, 15)
                },
                bounds: Vec::new(),
                body: Vec::new(),
                location: cols(1, 18),
                trait_instance: None,
                class_instance: None,
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
                    source: None,
                    resolved_type: types::TypeRef::Unknown,
                    name: Constant {
                        name: "A".to_string(),
                        location: cols(6, 6)
                    },
                    arguments: Vec::new(),
                    location: cols(6, 6)
                },
                class_name: Constant {
                    name: "B".to_string(),
                    location: cols(12, 12)
                },
                bounds: vec![TypeBound {
                    name: Constant {
                        name: "T".to_string(),
                        location: cols(17, 17)
                    },
                    requirements: vec![
                        Requirement::Trait(TypeName {
                            source: None,
                            resolved_type: types::TypeRef::Unknown,
                            name: Constant {
                                name: "X".to_string(),
                                location: cols(20, 20)
                            },
                            arguments: Vec::new(),
                            location: cols(20, 20)
                        }),
                        Requirement::Mutable(cols(24, 26))
                    ],
                    location: cols(17, 26)
                }],
                body: Vec::new(),
                location: cols(1, 29),
                trait_instance: None,
                class_instance: None,
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
                    source: None,
                    resolved_type: types::TypeRef::Unknown,
                    name: Constant {
                        name: "A".to_string(),
                        location: cols(6, 6)
                    },
                    arguments: Vec::new(),
                    location: cols(6, 6)
                },
                class_name: Constant {
                    name: "B".to_string(),
                    location: cols(12, 12)
                },
                bounds: Vec::new(),
                body: vec![DefineInstanceMethod {
                    public: false,
                    kind: MethodKind::Regular,
                    name: Identifier {
                        name: "foo".to_string(),
                        location: cols(19, 21)
                    },
                    type_parameters: Vec::new(),
                    arguments: Vec::new(),
                    throw_type: None,
                    return_type: None,
                    body: Vec::new(),
                    method_id: None,
                    location: cols(16, 24)
                }],
                location: cols(1, 26),
                trait_instance: None,
                class_instance: None,
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
                    source: None,
                    resolved_type: types::TypeRef::Unknown,
                    name: Constant {
                        name: "A".to_string(),
                        location: cols(6, 6)
                    },
                    arguments: Vec::new(),
                    location: cols(6, 6)
                },
                class_name: Constant {
                    name: "B".to_string(),
                    location: cols(12, 12)
                },
                bounds: Vec::new(),
                body: vec![DefineInstanceMethod {
                    public: false,
                    kind: MethodKind::Moving,
                    name: Identifier {
                        name: "foo".to_string(),
                        location: cols(24, 26)
                    },
                    type_parameters: Vec::new(),
                    arguments: Vec::new(),
                    throw_type: None,
                    return_type: None,
                    body: Vec::new(),
                    method_id: None,
                    location: cols(16, 29)
                }],
                location: cols(1, 31),
                trait_instance: None,
                class_instance: None,
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
    fn test_lower_import_symbol() {
        let hir = lower_top_expr("import a::(b)").0;

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
                        location: cols(12, 12)
                    },
                    import_as: Identifier {
                        name: "b".to_string(),
                        location: cols(12, 12)
                    },
                    location: cols(12, 12)
                }],
                location: cols(1, 13)
            }))
        );
    }

    #[test]
    fn test_lower_import_symbol_with_alias() {
        let hir = lower_top_expr("import a::(b as c)").0;

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
                        location: cols(12, 12)
                    },
                    import_as: Identifier {
                        name: "c".to_string(),
                        location: cols(17, 17)
                    },
                    location: cols(12, 17)
                }],
                location: cols(1, 18)
            }))
        );
    }

    #[test]
    fn test_lower_import_self() {
        let hir = lower_top_expr("import a::(self)").0;

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
                        location: cols(12, 15)
                    },
                    import_as: Identifier {
                        name: "self".to_string(),
                        location: cols(12, 15)
                    },
                    location: cols(12, 15)
                }],
                location: cols(1, 16)
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
                values: vec![StringValue::Text(Box::new(StringText {
                    value: "a".to_string(),
                    location: cols(9, 9)
                }))],
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
                values: vec![StringValue::Text(Box::new(StringText {
                    value: "a".to_string(),
                    location: cols(9, 9)
                }))],
                resolved_type: types::TypeRef::Unknown,
                location: cols(8, 10)
            }))
        );
    }

    #[test]
    fn test_lower_double_string_with_interpolation() {
        let hir = lower_expr("fn a { \"a{10}b\" }").0;

        assert_eq!(
            hir,
            Expression::String(Box::new(StringLiteral {
                values: vec![
                    StringValue::Text(Box::new(StringText {
                        value: "a".to_string(),
                        location: cols(9, 9)
                    })),
                    StringValue::Expression(Box::new(Call {
                        kind: types::CallKind::Unknown,
                        receiver: Some(Expression::Int(Box::new(IntLiteral {
                            value: 10,
                            resolved_type: types::TypeRef::Unknown,
                            location: cols(11, 12)
                        }))),
                        name: Identifier {
                            name: types::TO_STRING_METHOD.to_string(),
                            location: cols(11, 12)
                        },
                        arguments: Vec::new(),
                        else_block: None,
                        location: cols(11, 12)
                    })),
                    StringValue::Text(Box::new(StringText {
                        value: "b".to_string(),
                        location: cols(14, 14)
                    }))
                ],
                resolved_type: types::TypeRef::Unknown,
                location: cols(8, 15)
            }))
        );
    }

    #[test]
    fn test_lower_array() {
        let hir = lower_expr("fn a { [10] }").0;

        assert_eq!(
            hir,
            Expression::Array(Box::new(ArrayLiteral {
                value_type: types::TypeRef::Unknown,
                resolved_type: types::TypeRef::Unknown,
                values: vec![Expression::Int(Box::new(IntLiteral {
                    resolved_type: types::TypeRef::Unknown,
                    value: 10,
                    location: cols(9, 10)
                }))],
                location: cols(8, 11)
            }))
        );
    }

    #[test]
    fn test_lower_tuple() {
        let hir = lower_expr("fn a { (10,) }").0;

        assert_eq!(
            hir,
            Expression::Tuple(Box::new(TupleLiteral {
                class_id: None,
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
                arguments: vec![Argument::Positional(Box::new(
                    Expression::Int(Box::new(IntLiteral {
                        value: 2,
                        resolved_type: types::TypeRef::Unknown,
                        location: cols(12, 12)
                    }))
                ))],
                name: Identifier {
                    name: Operator::Add.method_name().to_string(),
                    location: cols(10, 10)
                },
                else_block: None,
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
                field_id: None,
                name: "a".to_string(),
                resolved_type: types::TypeRef::Unknown,
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
                location: cols(8, 8)
            }))
        );
    }

    #[test]
    fn test_lower_namespaced_constant() {
        let hir = lower_expr("fn a { a::B }").0;

        assert_eq!(
            hir,
            Expression::ConstantRef(Box::new(ConstantRef {
                kind: types::ConstantKind::Unknown,
                source: Some(Identifier {
                    name: "a".to_string(),
                    location: cols(8, 8)
                }),
                name: "B".to_string(),
                resolved_type: types::TypeRef::Unknown,
                location: cols(11, 11)
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
                arguments: vec![Argument::Positional(Box::new(
                    Expression::Int(Box::new(IntLiteral {
                        value: 10,
                        resolved_type: types::TypeRef::Unknown,
                        location: cols(10, 11)
                    }))
                ))],
                else_block: None,
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
                    location: cols(10, 14)
                }))],
                else_block: None,
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
                        location: cols(8, 8)
                    }
                ))),
                name: Identifier {
                    name: "b".to_string(),
                    location: cols(10, 10)
                },
                arguments: Vec::new(),
                else_block: None,
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
                else_block: None,
                location: cols(8, 20)
            }))
        );
    }

    #[test]
    fn test_lower_try_builtin_call() {
        let hir = lower_expr("fn a { try _INKO.foo(10) else 0 }").0;

        assert_eq!(
            hir,
            Expression::BuiltinCall(Box::new(BuiltinCall {
                info: None,
                name: Identifier {
                    name: "foo".to_string(),
                    location: cols(18, 20)
                },
                arguments: vec![Expression::Int(Box::new(IntLiteral {
                    value: 10,
                    resolved_type: types::TypeRef::Unknown,
                    location: cols(22, 23)
                }))],
                else_block: Some(ElseBlock {
                    body: vec![Expression::Int(Box::new(IntLiteral {
                        resolved_type: types::TypeRef::Unknown,
                        value: 0,
                        location: cols(31, 31)
                    }))],
                    argument: None,
                    location: cols(26, 31)
                }),
                location: cols(8, 31)
            }))
        );
    }

    #[test]
    fn test_lower_try_builtin_call_outside_stdlib() {
        let name = ModuleName::new("foo");
        let ast = Parser::new(
            "fn a { try _INKO.foo(10) else 0 }".into(),
            "test.inko".into(),
        )
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
                else_block: None,
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
                            location: cols(8, 8)
                        }
                    ))),
                    arguments: vec![Argument::Positional(Box::new(
                        Expression::Int(Box::new(IntLiteral {
                            value: 1,
                            resolved_type: types::TypeRef::Unknown,
                            location: cols(13, 13)
                        }))
                    ))],
                    name: Identifier {
                        name: Operator::Add.method_name().to_string(),
                        location: cols(10, 11)
                    },
                    else_block: None,
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
                else_block: None,
                location: cols(8, 14)
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
                                location: cols(8, 8)
                            }
                        ))),
                        name: Identifier {
                            name: "b".to_string(),
                            location: cols(10, 10)
                        },
                        arguments: Vec::new(),
                        else_block: None,
                        location: cols(8, 10)
                    }))),
                    arguments: vec![Argument::Positional(Box::new(
                        Expression::Int(Box::new(IntLiteral {
                            resolved_type: types::TypeRef::Unknown,
                            value: 1,
                            location: cols(15, 15)
                        }))
                    ))],
                    name: Identifier {
                        name: Operator::Add.method_name().to_string(),
                        location: cols(12, 13)
                    },
                    else_block: None,
                    location: cols(8, 15)
                })),
                else_block: None,
                location: cols(8, 15)
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
                        field_id: None,
                        name: "a".to_string(),
                        resolved_type: types::TypeRef::Unknown,
                        location: cols(8, 9)
                    }))),
                    arguments: vec![Argument::Positional(Box::new(
                        Expression::Int(Box::new(IntLiteral {
                            value: 1,
                            resolved_type: types::TypeRef::Unknown,
                            location: cols(14, 14)
                        }))
                    ))],
                    name: Identifier {
                        name: Operator::Add.method_name().to_string(),
                        location: cols(11, 12)
                    },
                    else_block: None,
                    location: cols(8, 14)
                })),
                resolved_type: types::TypeRef::Unknown,
                location: cols(8, 14)
            }))
        );
    }

    #[test]
    fn test_lower_closure() {
        let hir = lower_expr("fn a { fn (a: T) !! A -> B { 10 } }").0;

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
                throw_type: Some(Type::Named(Box::new(TypeName {
                    source: None,
                    resolved_type: types::TypeRef::Unknown,
                    name: Constant {
                        name: "A".to_string(),
                        location: cols(21, 21)
                    },
                    arguments: Vec::new(),
                    location: cols(21, 21)
                }))),
                return_type: Some(Type::Named(Box::new(TypeName {
                    source: None,
                    resolved_type: types::TypeRef::Unknown,
                    name: Constant {
                        name: "B".to_string(),
                        location: cols(26, 26)
                    },
                    arguments: Vec::new(),
                    location: cols(26, 26)
                }))),
                body: vec![Expression::Int(Box::new(IntLiteral {
                    value: 10,
                    resolved_type: types::TypeRef::Unknown,
                    location: cols(30, 31)
                }))],
                location: cols(8, 33)
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
    fn test_lower_index_expression() {
        let hir = lower_expr("fn a { a[0] }").0;

        assert_eq!(
            hir,
            Expression::Index(Box::new(Index {
                info: None,
                receiver: Expression::IdentifierRef(Box::new(IdentifierRef {
                    kind: types::IdentifierKind::Unknown,
                    name: "a".to_string(),
                    location: cols(8, 8)
                })),
                index: Expression::Int(Box::new(IntLiteral {
                    value: 0,
                    resolved_type: types::TypeRef::Unknown,
                    location: cols(10, 10)
                })),
                location: cols(8, 11)
            }))
        );
    }

    #[test]
    fn test_lower_set_index_expression() {
        let hir = lower_expr("fn a { a[0] = 1 }").0;

        assert_eq!(
            hir,
            Expression::Call(Box::new(Call {
                kind: types::CallKind::Unknown,
                receiver: Some(Expression::IdentifierRef(Box::new(
                    IdentifierRef {
                        kind: types::IdentifierKind::Unknown,
                        name: "a".to_string(),
                        location: cols(8, 8)
                    }
                ))),
                name: Identifier {
                    name: SET_INDEX_METHOD.to_string(),
                    location: cols(8, 15)
                },
                arguments: vec![
                    Argument::Positional(Box::new(Expression::Int(Box::new(
                        IntLiteral {
                            value: 0,
                            resolved_type: types::TypeRef::Unknown,
                            location: cols(10, 10)
                        }
                    )))),
                    Argument::Positional(Box::new(Expression::Int(Box::new(
                        IntLiteral {
                            value: 1,
                            resolved_type: types::TypeRef::Unknown,
                            location: cols(15, 15)
                        }
                    )))),
                ],
                else_block: None,
                location: cols(8, 15)
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
    fn test_lower_try_without_else() {
        let hir = lower_expr("fn a { try a }").0;

        assert_eq!(
            hir,
            Expression::Call(Box::new(Call {
                kind: types::CallKind::Unknown,
                receiver: None,
                name: Identifier {
                    name: "a".to_string(),
                    location: cols(12, 12)
                },
                arguments: Vec::new(),
                else_block: Some(ElseBlock {
                    body: vec![Expression::Throw(Box::new(Throw {
                        resolved_type: types::TypeRef::Unknown,
                        value: Expression::IdentifierRef(Box::new(
                            IdentifierRef {
                                kind: types::IdentifierKind::Unknown,
                                name: TRY_BINDING_VAR.to_string(),
                                location: cols(8, 12)
                            }
                        )),
                        location: cols(8, 12)
                    }))],
                    argument: Some(BlockArgument {
                        variable_id: None,
                        name: Identifier {
                            name: TRY_BINDING_VAR.to_string(),
                            location: cols(8, 12)
                        },
                        value_type: None,
                        location: cols(8, 12)
                    }),
                    location: cols(8, 12)
                }),
                location: cols(8, 12)
            }))
        );
    }

    #[test]
    fn test_lower_try_with_else() {
        let hir = lower_expr("fn a { try aa else (e) throw e }").0;

        assert_eq!(
            hir,
            Expression::Call(Box::new(Call {
                kind: types::CallKind::Unknown,
                receiver: None,
                name: Identifier {
                    name: "aa".to_string(),
                    location: cols(12, 13)
                },
                arguments: Vec::new(),
                else_block: Some(ElseBlock {
                    body: vec![Expression::Throw(Box::new(Throw {
                        resolved_type: types::TypeRef::Unknown,
                        value: Expression::IdentifierRef(Box::new(
                            IdentifierRef {
                                kind: types::IdentifierKind::Unknown,
                                name: "e".to_string(),
                                location: cols(30, 30)
                            }
                        )),
                        location: cols(24, 30)
                    }))],
                    argument: Some(BlockArgument {
                        variable_id: None,
                        name: Identifier {
                            name: "e".to_string(),
                            location: cols(21, 21)
                        },
                        value_type: None,
                        location: cols(21, 21)
                    }),
                    location: cols(15, 30)
                }),
                location: cols(8, 30)
            }))
        );
    }

    #[test]
    fn test_lower_try_panic() {
        let hir = lower_expr("fn a { try! aa }").0;

        assert_eq!(
            hir,
            Expression::Call(Box::new(Call {
                kind: types::CallKind::Unknown,
                receiver: None,
                name: Identifier {
                    name: "aa".to_string(),
                    location: cols(13, 14)
                },
                arguments: Vec::new(),
                else_block: Some(ElseBlock {
                    body: vec![Expression::BuiltinCall(Box::new(
                        BuiltinCall {
                            info: None,
                            name: Identifier {
                                name: types::BuiltinFunction::PanicThrown
                                    .name()
                                    .to_string(),
                                location: cols(8, 14)
                            },
                            arguments: vec![Expression::IdentifierRef(
                                Box::new(IdentifierRef {
                                    kind: types::IdentifierKind::Unknown,
                                    name: TRY_BINDING_VAR.to_string(),
                                    location: cols(8, 14)
                                })
                            )],
                            else_block: None,
                            location: cols(8, 14)
                        }
                    ))],
                    argument: Some(BlockArgument {
                        variable_id: None,
                        name: Identifier {
                            name: TRY_BINDING_VAR.to_string(),
                            location: cols(8, 14)
                        },
                        value_type: None,
                        location: cols(8, 14)
                    }),
                    location: cols(8, 14)
                }),
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
    fn test_lower_instance_literal() {
        let hir = lower_expr("fn a { A { @a = 10 } }").0;

        assert_eq!(
            hir,
            Expression::ClassLiteral(Box::new(ClassLiteral {
                class_id: None,
                resolved_type: types::TypeRef::Unknown,
                class_name: Constant {
                    name: "A".to_string(),
                    location: cols(8, 8)
                },
                fields: vec![AssignInstanceLiteralField {
                    resolved_type: types::TypeRef::Unknown,
                    field_id: None,
                    field: Field {
                        name: "a".to_string(),
                        location: cols(12, 13)
                    },
                    value: Expression::Int(Box::new(IntLiteral {
                        value: 10,
                        resolved_type: types::TypeRef::Unknown,
                        location: cols(17, 18)
                    })),
                    location: cols(12, 18)
                }],
                location: cols(8, 20)
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
    fn test_lower_enum_class() {
        let hir =
            lower_top_expr("class enum Option[T] { case Some(T) case None }").0;

        assert_eq!(
            hir,
            TopLevelExpression::Class(Box::new(DefineClass {
                public: false,
                kind: ClassKind::Enum,
                class_id: None,
                name: Constant {
                    name: "Option".to_string(),
                    location: cols(12, 17),
                },
                type_parameters: vec![TypeParameter {
                    type_parameter_id: None,
                    name: Constant {
                        name: "T".to_string(),
                        location: cols(19, 19)
                    },
                    requirements: Vec::new(),
                    location: cols(19, 19)
                }],
                body: vec![
                    ClassExpression::Variant(Box::new(DefineVariant {
                        method_id: None,
                        variant_id: None,
                        name: Constant {
                            name: "Some".to_string(),
                            location: cols(29, 32)
                        },
                        members: vec![Type::Named(Box::new(TypeName {
                            source: None,
                            resolved_type: types::TypeRef::Unknown,
                            name: Constant {
                                name: "T".to_string(),
                                location: cols(34, 34)
                            },
                            arguments: Vec::new(),
                            location: cols(34, 34)
                        }))],
                        location: cols(24, 35)
                    },)),
                    ClassExpression::Variant(Box::new(DefineVariant {
                        method_id: None,
                        variant_id: None,
                        name: Constant {
                            name: "None".to_string(),
                            location: cols(42, 45)
                        },
                        members: Vec::new(),
                        location: cols(37, 40)
                    },))
                ],
                location: cols(1, 47)
            }))
        );
    }
}
