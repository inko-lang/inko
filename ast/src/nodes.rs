use crate::lexer::Token;
use crate::source_location::SourceLocation;
use std::cmp::{Eq, PartialEq};
use std::path::PathBuf;

pub trait Node {
    fn location(&self) -> &SourceLocation;
}

#[derive(Debug, PartialEq, Eq)]
pub struct IntLiteral {
    pub value: String,
    pub location: SourceLocation,
}

impl From<Token> for IntLiteral {
    fn from(token: Token) -> Self {
        Self { value: token.value, location: token.location }
    }
}

impl Node for IntLiteral {
    fn location(&self) -> &SourceLocation {
        &self.location
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct FloatLiteral {
    pub value: String,
    pub location: SourceLocation,
}

impl Node for FloatLiteral {
    fn location(&self) -> &SourceLocation {
        &self.location
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct StringText {
    pub value: String,
    pub location: SourceLocation,
}

impl Node for StringText {
    fn location(&self) -> &SourceLocation {
        &self.location
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct StringLiteral {
    pub value: Option<StringText>,
    pub location: SourceLocation,
}

impl Node for StringLiteral {
    fn location(&self) -> &SourceLocation {
        &self.location
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct StringExpression {
    pub value: Expression,
    pub location: SourceLocation,
}

impl Node for StringExpression {
    fn location(&self) -> &SourceLocation {
        &self.location
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct UnicodeEscape {
    pub value: String,
    pub location: SourceLocation,
}

#[derive(Debug, PartialEq, Eq)]
pub enum DoubleStringValue {
    Text(Box<StringText>),
    Unicode(Box<UnicodeEscape>),
    Expression(Box<StringExpression>),
}

#[derive(Debug, PartialEq, Eq)]
pub struct DoubleStringLiteral {
    pub values: Vec<DoubleStringValue>,
    pub location: SourceLocation,
}

impl Node for DoubleStringLiteral {
    fn location(&self) -> &SourceLocation {
        &self.location
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Array {
    pub values: Vec<Expression>,
    pub location: SourceLocation,
}

impl Node for Array {
    fn location(&self) -> &SourceLocation {
        &self.location
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Tuple {
    pub values: Vec<Expression>,
    pub location: SourceLocation,
}

impl Node for Tuple {
    fn location(&self) -> &SourceLocation {
        &self.location
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Identifier {
    pub name: String,
    pub location: SourceLocation,
}

impl From<Token> for Identifier {
    fn from(token: Token) -> Self {
        Self { name: token.value, location: token.location }
    }
}

impl Node for Identifier {
    fn location(&self) -> &SourceLocation {
        &self.location
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Constant {
    pub source: Option<Identifier>,
    pub name: String,
    pub location: SourceLocation,
}

impl From<Token> for Constant {
    fn from(token: Token) -> Self {
        Self { source: None, name: token.value, location: token.location }
    }
}

impl Node for Constant {
    fn location(&self) -> &SourceLocation {
        &self.location
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Call {
    pub receiver: Option<Expression>,
    pub name: Identifier,
    pub arguments: Option<Arguments>,
    pub location: SourceLocation,
}

impl Node for Call {
    fn location(&self) -> &SourceLocation {
        &self.location
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct AssignVariable {
    pub variable: Identifier,
    pub value: Expression,
    pub location: SourceLocation,
}

impl Node for AssignVariable {
    fn location(&self) -> &SourceLocation {
        &self.location
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct ReplaceVariable {
    pub variable: Identifier,
    pub value: Expression,
    pub location: SourceLocation,
}

impl Node for ReplaceVariable {
    fn location(&self) -> &SourceLocation {
        &self.location
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct AssignField {
    pub field: Field,
    pub value: Expression,
    pub location: SourceLocation,
}

impl Node for AssignField {
    fn location(&self) -> &SourceLocation {
        &self.location
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct ReplaceField {
    pub field: Field,
    pub value: Expression,
    pub location: SourceLocation,
}

impl Node for ReplaceField {
    fn location(&self) -> &SourceLocation {
        &self.location
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct AssignSetter {
    pub receiver: Expression,
    pub name: Identifier,
    pub value: Expression,
    pub location: SourceLocation,
}

impl Node for AssignSetter {
    fn location(&self) -> &SourceLocation {
        &self.location
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct BinaryAssignVariable {
    pub operator: Operator,
    pub variable: Identifier,
    pub value: Expression,
    pub location: SourceLocation,
}

impl Node for BinaryAssignVariable {
    fn location(&self) -> &SourceLocation {
        &self.location
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct BinaryAssignField {
    pub operator: Operator,
    pub field: Field,
    pub value: Expression,
    pub location: SourceLocation,
}

impl Node for BinaryAssignField {
    fn location(&self) -> &SourceLocation {
        &self.location
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct BinaryAssignSetter {
    pub operator: Operator,
    pub receiver: Expression,
    pub name: Identifier,
    pub value: Expression,
    pub location: SourceLocation,
}

impl Node for BinaryAssignSetter {
    fn location(&self) -> &SourceLocation {
        &self.location
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct ImportAlias {
    pub name: String,
    pub location: SourceLocation,
}

impl Node for ImportAlias {
    fn location(&self) -> &SourceLocation {
        &self.location
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct ImportSymbol {
    pub name: String,
    pub alias: Option<ImportAlias>,
    pub location: SourceLocation,
}

impl Node for ImportSymbol {
    fn location(&self) -> &SourceLocation {
        &self.location
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct ImportSymbols {
    pub values: Vec<ImportSymbol>,
    pub location: SourceLocation,
}

impl Node for ImportSymbols {
    fn location(&self) -> &SourceLocation {
        &self.location
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct ImportPath {
    pub steps: Vec<Identifier>,
    pub location: SourceLocation,
}

impl Node for ImportPath {
    fn location(&self) -> &SourceLocation {
        &self.location
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct BuildTags {
    pub values: Vec<Identifier>,
    pub location: SourceLocation,
}

impl Node for BuildTags {
    fn location(&self) -> &SourceLocation {
        &self.location
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Import {
    pub path: ImportPath,
    pub symbols: Option<ImportSymbols>,
    pub location: SourceLocation,
    pub tags: Option<BuildTags>,
    pub include: bool,
}

impl Node for Import {
    fn location(&self) -> &SourceLocation {
        &self.location
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct ExternImportPath {
    pub path: String,
    pub location: SourceLocation,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ExternImport {
    pub path: ExternImportPath,
    pub location: SourceLocation,
}

impl Node for ExternImport {
    fn location(&self) -> &SourceLocation {
        &self.location
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct DefineConstant {
    pub public: bool,
    pub name: Constant,
    pub value: Expression,
    pub location: SourceLocation,
}

impl Node for DefineConstant {
    fn location(&self) -> &SourceLocation {
        &self.location
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum MethodKind {
    Instance,
    Static,
    Async,
    Moving,
    Mutable,
    AsyncMutable,
    Extern,
}

#[derive(Debug, PartialEq, Eq)]
pub struct DefineMethod {
    pub public: bool,
    pub kind: MethodKind,
    pub operator: bool,
    pub name: Identifier,
    pub type_parameters: Option<TypeParameters>,
    pub arguments: Option<MethodArguments>,
    pub return_type: Option<Type>,
    pub body: Option<Expressions>,
    pub location: SourceLocation,
}

impl Node for DefineMethod {
    fn location(&self) -> &SourceLocation {
        &self.location
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct DefineField {
    pub public: bool,
    pub name: Identifier,
    pub value_type: Type,
    pub location: SourceLocation,
}

impl Node for DefineField {
    fn location(&self) -> &SourceLocation {
        &self.location
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum ClassExpression {
    DefineMethod(Box<DefineMethod>),
    DefineField(Box<DefineField>),
    DefineVariant(Box<DefineVariant>),
}

#[derive(Debug, PartialEq, Eq)]
pub struct ClassExpressions {
    pub values: Vec<ClassExpression>,
    pub location: SourceLocation,
}

impl Node for ClassExpressions {
    fn location(&self) -> &SourceLocation {
        &self.location
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum ClassKind {
    Async,
    Builtin,
    Enum,
    Regular,
    Extern,
}

#[derive(Debug, PartialEq, Eq)]
pub struct DefineClass {
    pub public: bool,
    pub kind: ClassKind,
    pub name: Constant,
    pub type_parameters: Option<TypeParameters>,
    pub body: ClassExpressions,
    pub location: SourceLocation,
}

impl Node for DefineClass {
    fn location(&self) -> &SourceLocation {
        &self.location
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct DefineVariant {
    pub name: Constant,
    pub members: Option<Types>,
    pub location: SourceLocation,
}

#[derive(Debug, PartialEq, Eq)]
pub struct AssignInstanceLiteralField {
    pub field: Field,
    pub value: Expression,
    pub location: SourceLocation,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ClassLiteral {
    pub class_name: Constant,
    pub fields: Vec<AssignInstanceLiteralField>,
    pub location: SourceLocation,
}

impl Node for ClassLiteral {
    fn location(&self) -> &SourceLocation {
        &self.location
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct TraitExpressions {
    pub values: Vec<DefineMethod>,
    pub location: SourceLocation,
}

impl Node for TraitExpressions {
    fn location(&self) -> &SourceLocation {
        &self.location
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct DefineTrait {
    pub public: bool,
    pub name: Constant,
    pub type_parameters: Option<TypeParameters>,
    pub requirements: Option<TypeNames>,
    pub body: TraitExpressions,
    pub location: SourceLocation,
}

impl Node for DefineTrait {
    fn location(&self) -> &SourceLocation {
        &self.location
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum TopLevelExpression {
    DefineConstant(Box<DefineConstant>),
    DefineMethod(Box<DefineMethod>),
    DefineClass(Box<DefineClass>),
    DefineTrait(Box<DefineTrait>),
    ReopenClass(Box<ReopenClass>),
    ImplementTrait(Box<ImplementTrait>),
    Import(Box<Import>),
    ExternImport(Box<ExternImport>),
}

impl Node for TopLevelExpression {
    fn location(&self) -> &SourceLocation {
        match self {
            TopLevelExpression::DefineConstant(ref typ) => typ.location(),
            TopLevelExpression::DefineMethod(ref typ) => typ.location(),
            TopLevelExpression::DefineClass(ref typ) => typ.location(),
            TopLevelExpression::DefineTrait(ref typ) => typ.location(),
            TopLevelExpression::ReopenClass(ref typ) => typ.location(),
            TopLevelExpression::ImplementTrait(ref typ) => typ.location(),
            TopLevelExpression::Import(ref typ) => typ.location(),
            TopLevelExpression::ExternImport(ref typ) => typ.location(),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct ImplementationExpressions {
    pub values: Vec<DefineMethod>,
    pub location: SourceLocation,
}

impl Node for ImplementationExpressions {
    fn location(&self) -> &SourceLocation {
        &self.location
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct ReopenClass {
    pub class_name: Constant,
    pub body: ImplementationExpressions,
    pub location: SourceLocation,
    pub bounds: Option<TypeBounds>,
}

impl Node for ReopenClass {
    fn location(&self) -> &SourceLocation {
        &self.location
    }
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, PartialEq, Eq)]
pub enum Requirement {
    Trait(TypeName),
    Mutable(SourceLocation),
}

impl Node for Requirement {
    fn location(&self) -> &SourceLocation {
        match self {
            Requirement::Trait(n) => &n.location,
            Requirement::Mutable(loc) => loc,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Requirements {
    pub values: Vec<Requirement>,
    pub location: SourceLocation,
}

impl Node for Requirements {
    fn location(&self) -> &SourceLocation {
        &self.location
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct TypeBound {
    pub name: Constant,
    pub requirements: Requirements,
    pub location: SourceLocation,
}

impl Node for TypeBound {
    fn location(&self) -> &SourceLocation {
        &self.location
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct TypeBounds {
    pub values: Vec<TypeBound>,
    pub location: SourceLocation,
}

impl Node for TypeBounds {
    fn location(&self) -> &SourceLocation {
        &self.location
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct ImplementTrait {
    pub trait_name: TypeName,
    pub class_name: Constant,
    pub body: ImplementationExpressions,
    pub location: SourceLocation,
    pub bounds: Option<TypeBounds>,
}

impl Node for ImplementTrait {
    fn location(&self) -> &SourceLocation {
        &self.location
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Scope {
    pub body: Expressions,
    pub location: SourceLocation,
}

impl Node for Scope {
    fn location(&self) -> &SourceLocation {
        &self.location
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Expression {
    Int(Box<IntLiteral>),
    SingleString(Box<StringLiteral>),
    DoubleString(Box<DoubleStringLiteral>),
    Float(Box<FloatLiteral>),
    Binary(Box<Binary>),
    Field(Box<Field>),
    Constant(Box<Constant>),
    Identifier(Box<Identifier>),
    Call(Box<Call>),
    AssignVariable(Box<AssignVariable>),
    ReplaceVariable(Box<ReplaceVariable>),
    AssignField(Box<AssignField>),
    ReplaceField(Box<ReplaceField>),
    AssignSetter(Box<AssignSetter>),
    BinaryAssignVariable(Box<BinaryAssignVariable>),
    BinaryAssignField(Box<BinaryAssignField>),
    BinaryAssignSetter(Box<BinaryAssignSetter>),
    Closure(Box<Closure>),
    DefineVariable(Box<DefineVariable>),
    SelfObject(Box<SelfObject>),
    Group(Box<Group>),
    Next(Box<Next>),
    Break(Box<Break>),
    Ref(Box<Ref>),
    Mut(Box<Mut>),
    Recover(Box<Recover>),
    And(Box<And>),
    Or(Box<Or>),
    TypeCast(Box<TypeCast>),
    Throw(Box<Throw>),
    Return(Box<Return>),
    Try(Box<Try>),
    If(Box<If>),
    Match(Box<Match>),
    Loop(Box<Loop>),
    While(Box<While>),
    True(Box<True>),
    False(Box<False>),
    Nil(Box<Nil>),
    ClassLiteral(Box<ClassLiteral>),
    Scope(Box<Scope>),
    Array(Box<Array>),
    Tuple(Box<Tuple>),
}

impl Expression {
    pub fn boolean_and(left: Expression, right: Expression) -> Expression {
        let location =
            SourceLocation::start_end(left.location(), right.location());

        Expression::And(Box::new(And { left, right, location }))
    }

    pub fn boolean_or(left: Expression, right: Expression) -> Expression {
        let location =
            SourceLocation::start_end(left.location(), right.location());

        Expression::Or(Box::new(Or { left, right, location }))
    }
}

impl Node for Expression {
    fn location(&self) -> &SourceLocation {
        match self {
            Expression::And(ref typ) => typ.location(),
            Expression::Array(ref typ) => typ.location(),
            Expression::AssignField(ref typ) => typ.location(),
            Expression::ReplaceField(ref typ) => typ.location(),
            Expression::AssignSetter(ref typ) => typ.location(),
            Expression::AssignVariable(ref typ) => typ.location(),
            Expression::ReplaceVariable(ref typ) => typ.location(),
            Expression::Binary(ref typ) => typ.location(),
            Expression::BinaryAssignField(ref typ) => typ.location(),
            Expression::BinaryAssignSetter(ref typ) => typ.location(),
            Expression::BinaryAssignVariable(ref typ) => typ.location(),
            Expression::Break(ref typ) => typ.location(),
            Expression::Call(ref typ) => typ.location(),
            Expression::ClassLiteral(ref typ) => typ.location(),
            Expression::Closure(ref typ) => typ.location(),
            Expression::Constant(ref typ) => typ.location(),
            Expression::DefineVariable(ref typ) => typ.location(),
            Expression::DoubleString(ref typ) => typ.location(),
            Expression::False(ref typ) => typ.location(),
            Expression::Field(ref typ) => typ.location(),
            Expression::Float(ref typ) => typ.location(),
            Expression::Group(ref typ) => typ.location(),
            Expression::Identifier(ref typ) => typ.location(),
            Expression::If(ref typ) => typ.location(),
            Expression::Int(ref typ) => typ.location(),
            Expression::Loop(ref typ) => typ.location(),
            Expression::Match(ref typ) => typ.location(),
            Expression::Next(ref typ) => typ.location(),
            Expression::Or(ref typ) => typ.location(),
            Expression::Ref(ref typ) => typ.location(),
            Expression::Return(ref typ) => typ.location(),
            Expression::Scope(ref typ) => typ.location(),
            Expression::SelfObject(ref typ) => typ.location(),
            Expression::SingleString(ref typ) => typ.location(),
            Expression::Throw(ref typ) => typ.location(),
            Expression::True(ref typ) => typ.location(),
            Expression::Nil(ref typ) => typ.location(),
            Expression::Try(ref typ) => typ.location(),
            Expression::Tuple(ref typ) => typ.location(),
            Expression::TypeCast(ref typ) => typ.location(),
            Expression::While(ref typ) => typ.location(),
            Expression::Mut(ref typ) => typ.location(),
            Expression::Recover(ref typ) => typ.location(),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Types {
    pub values: Vec<Type>,
    pub location: SourceLocation,
}

impl Node for Types {
    fn location(&self) -> &SourceLocation {
        &self.location
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct TypeNames {
    pub values: Vec<TypeName>,
    pub location: SourceLocation,
}

impl Node for TypeNames {
    fn location(&self) -> &SourceLocation {
        &self.location
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct TypeParameter {
    pub name: Constant,
    pub requirements: Option<Requirements>,
    pub location: SourceLocation,
}

impl Node for TypeParameter {
    fn location(&self) -> &SourceLocation {
        &self.location
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct TypeParameters {
    pub values: Vec<TypeParameter>,
    pub location: SourceLocation,
}

impl Node for TypeParameters {
    fn location(&self) -> &SourceLocation {
        &self.location
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct MethodArgument {
    pub name: Identifier,
    pub value_type: Type,
    pub location: SourceLocation,
}

impl Node for MethodArgument {
    fn location(&self) -> &SourceLocation {
        &self.location
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct NamedArgument {
    pub name: Identifier,
    pub value: Expression,
    pub location: SourceLocation,
}

impl Node for NamedArgument {
    fn location(&self) -> &SourceLocation {
        &self.location
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Argument {
    Positional(Expression),
    Named(Box<NamedArgument>),
}

impl Node for Argument {
    fn location(&self) -> &SourceLocation {
        match self {
            Argument::Positional(ref typ) => typ.location(),
            Argument::Named(ref typ) => typ.location(),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Arguments {
    pub values: Vec<Argument>,
    pub location: SourceLocation,
}

impl Node for Arguments {
    fn location(&self) -> &SourceLocation {
        &self.location
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct MethodArguments {
    pub values: Vec<MethodArgument>,
    pub variadic: bool,
    pub location: SourceLocation,
}

impl Node for MethodArguments {
    fn location(&self) -> &SourceLocation {
        &self.location
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct TypeName {
    pub name: Constant,
    pub arguments: Option<Types>,
    pub location: SourceLocation,
}

impl Node for TypeName {
    fn location(&self) -> &SourceLocation {
        &self.location
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct ReferenceType {
    pub type_reference: ReferrableType,
    pub location: SourceLocation,
}

impl Node for ReferenceType {
    fn location(&self) -> &SourceLocation {
        &self.location
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct RcType {
    pub name: TypeName,
    pub location: SourceLocation,
}

impl Node for RcType {
    fn location(&self) -> &SourceLocation {
        &self.location
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum ReferrableType {
    Named(Box<TypeName>),
    Closure(Box<ClosureType>),
    Tuple(Box<TupleType>),
}

impl Node for ReferrableType {
    fn location(&self) -> &SourceLocation {
        match self {
            ReferrableType::Named(ref node) => node.location(),
            ReferrableType::Closure(ref node) => node.location(),
            ReferrableType::Tuple(ref node) => node.location(),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct ClosureType {
    pub arguments: Option<Types>,
    pub return_type: Option<Type>,
    pub location: SourceLocation,
}

impl Node for ClosureType {
    fn location(&self) -> &SourceLocation {
        &self.location
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct TupleType {
    pub values: Vec<Type>,
    pub location: SourceLocation,
}

impl Node for TupleType {
    fn location(&self) -> &SourceLocation {
        &self.location
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Type {
    Named(Box<TypeName>),
    Ref(Box<ReferenceType>),
    Mut(Box<ReferenceType>),
    Uni(Box<ReferenceType>),
    Owned(Box<ReferenceType>),
    Closure(Box<ClosureType>),
    Tuple(Box<TupleType>),
}

impl Node for Type {
    fn location(&self) -> &SourceLocation {
        match self {
            Type::Named(ref typ) => typ.location(),
            Type::Ref(ref typ) => typ.location(),
            Type::Mut(ref typ) => typ.location(),
            Type::Uni(ref typ) => typ.location(),
            Type::Owned(ref typ) => typ.location(),
            Type::Closure(ref typ) => typ.location(),
            Type::Tuple(ref typ) => typ.location(),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Expressions {
    pub values: Vec<Expression>,
    pub location: SourceLocation,
}

impl Node for Expressions {
    fn location(&self) -> &SourceLocation {
        &self.location
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum OperatorKind {
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

#[derive(Debug, PartialEq, Eq)]
pub struct Operator {
    pub kind: OperatorKind,
    pub location: SourceLocation,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Binary {
    pub left: Expression,
    pub right: Expression,
    pub operator: Operator,
    pub location: SourceLocation,
}

impl Node for Binary {
    fn location(&self) -> &SourceLocation {
        &self.location
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Field {
    pub name: String,
    pub location: SourceLocation,
}

impl From<Token> for Field {
    fn from(token: Token) -> Self {
        Self { name: token.value, location: token.location }
    }
}

impl Node for Field {
    fn location(&self) -> &SourceLocation {
        &self.location
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct BlockArgument {
    pub name: Identifier,
    pub value_type: Option<Type>,
    pub location: SourceLocation,
}

impl Node for BlockArgument {
    fn location(&self) -> &SourceLocation {
        &self.location
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct BlockArguments {
    pub values: Vec<BlockArgument>,
    pub location: SourceLocation,
}

impl Node for BlockArguments {
    fn location(&self) -> &SourceLocation {
        &self.location
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Closure {
    pub moving: bool,
    pub arguments: Option<BlockArguments>,
    pub return_type: Option<Type>,
    pub body: Expressions,
    pub location: SourceLocation,
}

impl Node for Closure {
    fn location(&self) -> &SourceLocation {
        &self.location
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct DefineElseBlock {
    pub body: Expressions,
    pub location: SourceLocation,
}

impl Node for DefineElseBlock {
    fn location(&self) -> &SourceLocation {
        &self.location
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct DefineVariable {
    pub mutable: bool,
    pub name: Identifier,
    pub value: Expression,
    pub value_type: Option<Type>,
    pub location: SourceLocation,
}

impl Node for DefineVariable {
    fn location(&self) -> &SourceLocation {
        &self.location
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct SelfObject {
    pub location: SourceLocation,
}

impl Node for SelfObject {
    fn location(&self) -> &SourceLocation {
        &self.location
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct True {
    pub location: SourceLocation,
}

impl Node for True {
    fn location(&self) -> &SourceLocation {
        &self.location
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Nil {
    pub location: SourceLocation,
}

impl Node for Nil {
    fn location(&self) -> &SourceLocation {
        &self.location
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct False {
    pub location: SourceLocation,
}

impl Node for False {
    fn location(&self) -> &SourceLocation {
        &self.location
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Next {
    pub location: SourceLocation,
}

impl Node for Next {
    fn location(&self) -> &SourceLocation {
        &self.location
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Break {
    pub location: SourceLocation,
}

impl Node for Break {
    fn location(&self) -> &SourceLocation {
        &self.location
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Group {
    pub value: Expression,
    pub location: SourceLocation,
}

impl Node for Group {
    fn location(&self) -> &SourceLocation {
        &self.location
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Ref {
    pub value: Expression,
    pub location: SourceLocation,
}

impl Node for Ref {
    fn location(&self) -> &SourceLocation {
        &self.location
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Mut {
    pub value: Expression,
    pub location: SourceLocation,
}

impl Node for Mut {
    fn location(&self) -> &SourceLocation {
        &self.location
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Recover {
    pub body: Expressions,
    pub location: SourceLocation,
}

impl Node for Recover {
    fn location(&self) -> &SourceLocation {
        &self.location
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct RcRef {
    pub value: Expression,
    pub location: SourceLocation,
}

impl Node for RcRef {
    fn location(&self) -> &SourceLocation {
        &self.location
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct And {
    pub left: Expression,
    pub right: Expression,
    pub location: SourceLocation,
}

impl Node for And {
    fn location(&self) -> &SourceLocation {
        &self.location
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Or {
    pub left: Expression,
    pub right: Expression,
    pub location: SourceLocation,
}

impl Node for Or {
    fn location(&self) -> &SourceLocation {
        &self.location
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct TypeCast {
    pub value: Expression,
    pub cast_to: Type,
    pub location: SourceLocation,
}

impl Node for TypeCast {
    fn location(&self) -> &SourceLocation {
        &self.location
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Throw {
    pub value: Expression,
    pub location: SourceLocation,
}

impl Node for Throw {
    fn location(&self) -> &SourceLocation {
        &self.location
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Return {
    pub value: Option<Expression>,
    pub location: SourceLocation,
}

impl Node for Return {
    fn location(&self) -> &SourceLocation {
        &self.location
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Try {
    pub expression: Expression,
    pub location: SourceLocation,
}

impl Node for Try {
    fn location(&self) -> &SourceLocation {
        &self.location
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct IfCondition {
    pub condition: Expression,
    pub body: Expressions,
    pub location: SourceLocation,
}

impl Node for IfCondition {
    fn location(&self) -> &SourceLocation {
        &self.location
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct If {
    pub if_true: IfCondition,
    pub else_if: Vec<IfCondition>,
    pub else_body: Option<Expressions>,
    pub location: SourceLocation,
}

impl Node for If {
    fn location(&self) -> &SourceLocation {
        &self.location
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct TuplePattern {
    pub values: Vec<Pattern>,
    pub location: SourceLocation,
}

#[derive(Debug, PartialEq, Eq)]
pub struct VariantPattern {
    pub name: Constant,
    pub values: Vec<Pattern>,
    pub location: SourceLocation,
}

#[derive(Debug, PartialEq, Eq)]
pub struct WildcardPattern {
    pub location: SourceLocation,
}

#[derive(Debug, PartialEq, Eq)]
pub struct IdentifierPattern {
    pub name: Identifier,
    pub mutable: bool,
    pub value_type: Option<Type>,
    pub location: SourceLocation,
}

#[derive(Debug, PartialEq, Eq)]
pub struct FieldPattern {
    pub field: Field,
    pub pattern: Pattern,
    pub location: SourceLocation,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ClassPattern {
    pub values: Vec<FieldPattern>,
    pub location: SourceLocation,
}

#[derive(Debug, PartialEq, Eq)]
pub struct OrPattern {
    pub patterns: Vec<Pattern>,
    pub location: SourceLocation,
}

#[derive(Debug, PartialEq, Eq)]
pub struct StringPattern {
    pub value: String,
    pub location: SourceLocation,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Pattern {
    Constant(Box<Constant>),
    Variant(Box<VariantPattern>),
    Class(Box<ClassPattern>),
    Expression(Box<Expression>),
    Identifier(Box<IdentifierPattern>),
    Tuple(Box<TuplePattern>),
    Wildcard(Box<WildcardPattern>),
    Or(Box<OrPattern>),
    String(Box<StringPattern>),
}

impl Pattern {
    pub fn location(&self) -> &SourceLocation {
        match self {
            Pattern::Constant(ref n) => &n.location,
            Pattern::Variant(ref n) => &n.location,
            Pattern::Class(ref n) => &n.location,
            Pattern::Expression(ref n) => n.location(),
            Pattern::Identifier(ref n) => &n.location,
            Pattern::Tuple(ref n) => &n.location,
            Pattern::Wildcard(ref n) => &n.location,
            Pattern::Or(ref n) => &n.location,
            Pattern::String(ref n) => &n.location,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct MatchCase {
    pub pattern: Pattern,
    pub guard: Option<Expression>,
    pub body: Expressions,
    pub location: SourceLocation,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Match {
    pub expression: Expression,
    pub cases: Vec<MatchCase>,
    pub location: SourceLocation,
}

impl Node for Match {
    fn location(&self) -> &SourceLocation {
        &self.location
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Loop {
    pub body: Expressions,
    pub location: SourceLocation,
}

impl Node for Loop {
    fn location(&self) -> &SourceLocation {
        &self.location
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct While {
    pub condition: Expression,
    pub body: Expressions,
    pub location: SourceLocation,
}

impl Node for While {
    fn location(&self) -> &SourceLocation {
        &self.location
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Module {
    pub expressions: Vec<TopLevelExpression>,
    pub file: PathBuf,
    pub location: SourceLocation,
}

impl Node for Module {
    fn location(&self) -> &SourceLocation {
        &self.location
    }
}
