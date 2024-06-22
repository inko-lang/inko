//! Pattern match compilation and exhaustiveness checking.
//!
//! This module provides types and methods for compiling `match` expressions
//! into decision trees, and checking them for exhaustiveness/redundant
//! patterns.
//!
//! # Algorithm
//!
//! Our implementation is based on
//! https://julesjacobs.com/notes/patternmatching/patternmatching.pdf and
//! https://github.com/yorickpeterse/pattern-matching-in-rust/tree/main/jacobs2021.
//!
//! The resulting decision tree doesn't contain AST or HIR nodes as the bodies
//! for match cases; instead we store basic block IDs.
//!
//! The match compiler introduces a type called "Variable". This type
//! essentially acts as a placeholder for a MIR register. Using these
//! placeholders allows compiling of decision trees regardless of how registers
//! are allocated, managed, etc. This also makes testing the match compiler in
//! isolation easier.
use crate::hir;
use crate::mir::{BlockId, Constant, Mir};
use crate::state::State;
use std::collections::{HashMap, HashSet};
use types::resolve::TypeResolver;
use types::{
    ClassInstance, ClassKind, Database, FieldId, TypeArguments, TypeBounds,
    TypeId, TypeRef, VariableId, VariantId, BOOL_ID, INT_ID, STRING_ID,
};

fn add_missing_patterns(
    db: &Database,
    node: &Decision,
    terms: &mut Vec<Term>,
    missing: &mut HashSet<String>,
) {
    match node {
        Decision::Success(_) => {}
        Decision::Fail => {
            let mut mapping = HashMap::new();

            // At this point the terms stack looks something like this:
            // `[term, term + arguments, term, ...]`. To construct a pattern
            // name from this stack, we first map all variables to their
            // term indexes. This is needed because when a term defines
            // arguments, the terms for those arguments don't necessarily
            // appear in order in the term stack.
            //
            // This mapping is then used when (recursively) generating a
            // pattern name.
            for (index, step) in terms.iter().enumerate() {
                mapping.insert(&step.variable, index);
            }

            let name = terms
                .first()
                .map(|term| term.pattern_name(terms, &mapping))
                .unwrap_or_else(|| "_".to_string());

            missing.insert(name);
        }
        Decision::Guard(_, _, fallback) => {
            add_missing_patterns(db, fallback, terms, missing);
        }
        Decision::Switch(var, cases, fallback) => {
            for case in cases {
                match &case.constructor {
                    Constructor::True => {
                        let name = "true".to_string();

                        terms.push(Term::new(*var, name, Vec::new()));
                    }
                    Constructor::False => {
                        let name = "false".to_string();

                        terms.push(Term::new(*var, name, Vec::new()));
                    }
                    Constructor::Int(_) | Constructor::String(_) => {
                        let name = "_".to_string();

                        terms.push(Term::new(*var, name, Vec::new()));
                    }
                    Constructor::Class(_) => {
                        let name = "_".to_string();

                        terms.push(Term::new(*var, name, Vec::new()));
                    }
                    Constructor::Tuple(_) => {
                        let name = String::new();
                        let args = case.arguments.clone();

                        terms.push(Term::new(*var, name, args));
                    }
                    Constructor::Variant(variant) => {
                        let args = case.arguments.clone();
                        let name = variant.name(db).clone();

                        terms.push(Term::new(*var, name, args));
                    }
                }

                add_missing_patterns(db, &case.node, terms, missing);
                terms.pop();
            }

            if let Some(node) = fallback {
                add_missing_patterns(db, node, terms, missing);
            }
        }
    }
}

/// Expands rows containing OR patterns into individual rows, such that each
/// branch in the OR produces its own row.
///
/// For each column that tests against an OR pattern, each sub pattern is
/// translated into a new row. This work repeats itself until no more OR
/// patterns remain in the rows.
fn expand_or_patterns(rows: &mut Vec<Row>) {
    // If none of the rows contain any OR patterns, we can avoid the below work
    // loop, saving some allocations and time.
    if !rows
        .iter()
        .any(|r| r.columns.iter().any(|c| matches!(c.pattern, Pattern::Or(_))))
    {
        return;
    }

    let mut new_rows = Vec::with_capacity(rows.len());
    let mut found = true;

    while found {
        found = false;

        for row in rows.drain(0..) {
            // Find the first column containing an OR pattern. We process this
            // one column at a time, as that's (much) easier to implement
            // compared to handling all columns at once (as multiple columns may
            // contain OR patterns).
            let res = row.columns.iter().enumerate().find_map(|(idx, col)| {
                if let Pattern::Or(pats) = &col.pattern {
                    Some((idx, col.variable, pats))
                } else {
                    None
                }
            });

            if let Some((idx, var, pats)) = res {
                found = true;

                for pat in pats {
                    let mut new_row = row.clone();

                    new_row.columns[idx] = Column::new(var, pat.clone());
                    new_rows.push(new_row);
                }
            } else {
                new_rows.push(row);
            }
        }

        std::mem::swap(rows, &mut new_rows);
    }
}

/// A binding to define as part of a pattern.
#[derive(Clone, Eq, PartialEq, Debug)]
pub(crate) enum Binding {
    Named(VariableId, Variable),

    /// Wildcards/ignored bindings don't actually result in bindings being
    /// created. Instead, we use this so we can distinguish objects matched
    /// against a wildcard from their parent objects. This is needed because
    /// objects matched against wildcards should be dropped as usual, while
    /// the parents of bound objects are dropped without running their dropper
    /// method.
    Ignored(Variable),
}

/// The body of a pattern matching arm/case to run.
#[derive(Clone, Eq, PartialEq, Debug)]
pub(crate) struct Body {
    /// Any variables to bind before running the code.
    pub(crate) bindings: Vec<Binding>,

    /// The basic block ID to jump to when a pattern matches.
    pub(crate) block_id: BlockId,
}

impl Body {
    pub(crate) fn new(block_id: BlockId) -> Self {
        Self { bindings: Vec::new(), block_id }
    }
}

/// A simplified description of a type, along with the data necessary to compile
/// its sub tree.
///
/// This structure is created from a `TypeRef` and allows us to re-use the same
/// compilation logic for different types. For example, both tuples and enums
/// translate to the `Type::Finite` variant.
#[derive(Debug)]
enum Type {
    Int,
    String,

    /// A type with a finite number of constructors, such as a tuple or enum.
    ///
    /// Each triple stores the following values:
    ///
    /// 1. The constructor to match against.
    /// 2. The variables/arguments to expose to the constructor sub tree.
    /// 3. An array for storing rows to use for building the constructor's sub
    ///    tree.
    Finite(Vec<(Constructor, Vec<Variable>, Vec<Row>)>),
}

/// A type constructor.
#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) enum Constructor {
    Class(Vec<FieldId>),
    False,
    Int(i64),
    String(String),
    True,
    Tuple(Vec<FieldId>),
    Variant(VariantId),
}

impl Constructor {
    /// Returns the index of this constructor relative to its type.
    fn index(&self, db: &Database) -> usize {
        match self {
            Constructor::False
            | Constructor::Int(_)
            | Constructor::String(_)
            | Constructor::Class(_)
            | Constructor::Tuple(_) => 0,
            Constructor::True => 1,
            Constructor::Variant(id) => id.id(db) as usize,
        }
    }
}

/// A user defined pattern such as `Some((x, 10))`.
#[derive(Clone, Eq, PartialEq, Debug)]
pub(crate) enum Pattern {
    Constructor(Constructor, Vec<Pattern>),
    Int(i64),
    String(String),
    Variable(VariableId),
    Or(Vec<Pattern>),
    Wildcard,
}

impl Pattern {
    pub(crate) fn from_hir(
        db: &Database,
        mir: &Mir,
        node: hir::Pattern,
    ) -> Self {
        match node {
            hir::Pattern::Class(n) => {
                let len = n.class_id.unwrap().number_of_fields(db);
                let mut args = vec![Pattern::Wildcard; len];
                let mut fields = vec![FieldId(0); len];

                for pat in n.values {
                    let field = pat.field_id.unwrap();
                    let index = field.index(db);

                    args[index] = Pattern::from_hir(db, mir, pat.pattern);
                    fields[index] = field;
                }

                Pattern::Constructor(Constructor::Class(fields), args)
            }
            hir::Pattern::Constant(n) => match n.kind {
                types::ConstantPatternKind::String(id) => {
                    match mir.constants.get(&id) {
                        Some(Constant::String(v)) => Pattern::String(v.clone()),
                        _ => unreachable!(),
                    }
                }
                types::ConstantPatternKind::Int(id) => {
                    match mir.constants.get(&id) {
                        Some(Constant::Int(v)) => Pattern::Int(*v),
                        _ => unreachable!(),
                    }
                }
                types::ConstantPatternKind::Variant(id) => {
                    Pattern::Constructor(Constructor::Variant(id), Vec::new())
                }
                types::ConstantPatternKind::Unknown => unreachable!(),
            },
            hir::Pattern::Identifier(n) => {
                Pattern::Variable(n.variable_id.unwrap())
            }
            hir::Pattern::Int(n) => Pattern::Int(n.value),
            hir::Pattern::String(n) => Pattern::String(n.value),
            hir::Pattern::Tuple(n) => {
                let args = n
                    .values
                    .into_iter()
                    .map(|p| Pattern::from_hir(db, mir, p))
                    .collect();

                Pattern::Constructor(Constructor::Tuple(n.field_ids), args)
            }
            hir::Pattern::Variant(n) => {
                let args = n
                    .values
                    .into_iter()
                    .map(|p| Pattern::from_hir(db, mir, p))
                    .collect();

                Pattern::Constructor(
                    Constructor::Variant(n.variant_id.unwrap()),
                    args,
                )
            }
            hir::Pattern::Wildcard(_) => Pattern::Wildcard,
            hir::Pattern::True(_) => {
                Pattern::Constructor(Constructor::True, Vec::new())
            }
            hir::Pattern::False(_) => {
                Pattern::Constructor(Constructor::False, Vec::new())
            }
            hir::Pattern::Or(n) => Pattern::Or(
                n.patterns
                    .into_iter()
                    .map(|p| Pattern::from_hir(db, mir, p))
                    .collect(),
            ),
        }
    }
}

/// A variable used in a match expression.
#[derive(Eq, PartialEq, Clone, Copy, Hash, Debug)]
pub(crate) struct Variable(pub(crate) usize);

impl Variable {
    pub(crate) fn value_type(self, variables: &Variables) -> TypeRef {
        variables.types[self.0]
    }
}

/// A single case (or row) in a match expression/table.
#[derive(Clone, Eq, PartialEq, Debug)]
pub(crate) struct Row {
    columns: Vec<Column>,
    guard: Option<hir::Expression>,
    body: Body,
}

impl Row {
    pub(crate) fn new(
        columns: Vec<Column>,
        guard: Option<hir::Expression>,
        body: Body,
    ) -> Self {
        Self { columns, guard, body }
    }

    fn remove_column(&mut self, variable: &Variable) -> Option<Column> {
        self.columns
            .iter()
            .position(|c| &c.variable == variable)
            .map(|idx| self.columns.remove(idx))
    }

    /// Moves variable-only patterns/tests into the right-hand side/body of a
    /// case.
    fn move_variable_patterns(&mut self) {
        self.columns.retain(|col| match &col.pattern {
            Pattern::Variable(id) => {
                self.body.bindings.push(Binding::Named(*id, col.variable));
                false
            }
            Pattern::Wildcard => {
                self.body.bindings.push(Binding::Ignored(col.variable));
                false
            }
            _ => true,
        });
    }
}

/// A column in a pattern matching table.
#[derive(Clone, Eq, PartialEq, Debug)]
pub(crate) struct Column {
    variable: Variable,
    pattern: Pattern,
}

impl Column {
    pub(crate) fn new(variable: Variable, pattern: Pattern) -> Self {
        Self { variable, pattern }
    }
}

/// A case in a decision tree to test against a variable.
#[derive(Eq, PartialEq, Debug)]
pub(crate) struct Case {
    /// The constructor to test against an input variable.
    pub(crate) constructor: Constructor,

    /// Variables to introduce to the body of this case.
    pub(crate) arguments: Vec<Variable>,

    /// The sub tree of this case.
    pub(crate) node: Decision,
}

impl Case {
    fn new(
        constructor: Constructor,
        arguments: Vec<Variable>,
        node: Decision,
    ) -> Self {
        Self { constructor, arguments, node }
    }
}

/// A decision tree compiled from a list of match cases.
#[derive(Eq, PartialEq, Debug)]
pub(crate) enum Decision {
    /// A pattern is matched and the right-hand value is to be returned.
    Success(Body),

    /// A pattern is missing.
    Fail,

    /// Checks if a guard evaluates to true, running the body if it does.
    ///
    /// The arguments are as follows:
    ///
    /// 1. The guard to evaluate.
    /// 2. The body to evaluate if the guard matches.
    /// 3. The sub tree to evaluate when the guard fails.
    Guard(hir::Expression, Body, Box<Decision>),

    /// Checks if a value is any of the given patterns.
    ///
    /// The values are as follows:
    ///
    /// 1. The variable to test.
    /// 2. The cases to test against this variable.
    /// 3. A fallback decision to take, in case none of the cases matched.
    Switch(Variable, Vec<Case>, Option<Box<Decision>>),
}

/// Information about a single constructor/value (aka term) being tested, used
/// to build a list of names of missing patterns.
#[derive(Debug)]
struct Term {
    variable: Variable,
    name: String,
    arguments: Vec<Variable>,
}

impl Term {
    fn new(variable: Variable, name: String, arguments: Vec<Variable>) -> Self {
        Self { variable, name, arguments }
    }

    fn pattern_name(
        &self,
        terms: &[Term],
        mapping: &HashMap<&Variable, usize>,
    ) -> String {
        if self.arguments.is_empty() {
            self.name.to_string()
        } else {
            let args = self
                .arguments
                .iter()
                .map(|arg| {
                    mapping
                        .get(&arg)
                        .map(|&idx| terms[idx].pattern_name(terms, mapping))
                        .unwrap_or_else(|| "_".to_string())
                })
                .collect::<Vec<_>>()
                .join(", ");

            format!("{}({})", self.name, args)
        }
    }
}

/// The result of compiling a pattern match expression.
pub(crate) struct Match {
    pub(crate) tree: Decision,
    pub(crate) missing: bool,
    pub(crate) variables: Variables,
}

impl Match {
    /// Returns a list of patterns not covered by the match expression.
    pub(crate) fn missing_patterns(&self, db: &Database) -> Vec<String> {
        let mut names = HashSet::new();
        let mut steps = Vec::new();

        add_missing_patterns(db, &self.tree, &mut steps, &mut names);

        let mut missing: Vec<String> = names.into_iter().collect();

        // Sorting isn't necessary, but it makes it a bit easier to write tests.
        missing.sort();
        missing
    }
}

/// State to use for creating pattern matching variables.
///
/// This state is separate from the compiler to work around a double borrowing
/// issue: a Compiler takes a `&mut State`, and we'd need a `Compiler` to create
/// the initial root/input variable. However, creating the rows to compile also
/// requires a `&mut State`, which Rust doesn't allow.
///
/// To work around this, the tracking of variable IDs is done using a separate
/// type we can create ahead of time to create the initial input/root variable.
pub(crate) struct Variables {
    id: usize,
    pub(crate) types: Vec<TypeRef>,
}

impl Variables {
    pub(crate) fn new() -> Self {
        Self { id: 0, types: Vec::new() }
    }

    pub(crate) fn new_variable(&mut self, value_type: TypeRef) -> Variable {
        let var = Variable(self.id);

        self.id += 1;

        self.types.push(value_type);
        var
    }
}

/// A type for compiling a HIR `match` expression into a decision tree.
pub(crate) struct Compiler<'a> {
    state: &'a mut State,

    /// The basic blocks that are reachable in the match expression.
    ///
    /// If a block isn't in this list it means its pattern is redundant.
    reachable: HashSet<BlockId>,

    /// A flag indicating one or more patterns are missing.
    missing: bool,

    /// The pattern matching variables/temporaries.
    variables: Variables,

    /// Type bounds to apply to types produced by patterns.
    bounds: TypeBounds,
}

impl<'a> Compiler<'a> {
    pub(crate) fn new(
        state: &'a mut State,
        variables: Variables,
        bounds: TypeBounds,
    ) -> Self {
        Self {
            state,
            reachable: HashSet::new(),
            missing: false,
            variables,
            bounds,
        }
    }

    pub(crate) fn compile(mut self, rows: Vec<Row>) -> Match {
        Match {
            tree: self.compile_rows(rows),
            missing: self.missing,
            variables: self.variables,
        }
    }

    pub(crate) fn new_variable(&mut self, value_type: TypeRef) -> Variable {
        self.variables.new_variable(value_type)
    }

    fn compile_rows(&mut self, mut rows: Vec<Row>) -> Decision {
        if rows.is_empty() {
            self.missing = true;

            return Decision::Fail;
        }

        expand_or_patterns(&mut rows);

        for row in &mut rows {
            row.move_variable_patterns();
        }

        // There may be multiple rows, but if the first one has no patterns
        // those extra rows are redundant, as a row without columns/patterns
        // always matches.
        if rows.first().map_or(false, |c| c.columns.is_empty()) {
            let row = rows.remove(0);

            self.reachable.insert(row.body.block_id);

            return if let Some(guard) = row.guard {
                Decision::Guard(
                    guard,
                    row.body,
                    Box::new(self.compile_rows(rows)),
                )
            } else {
                Decision::Success(row.body)
            };
        }

        let branch_var = self.branch_variable(&rows);

        match self.variable_type(&branch_var) {
            Type::Int => {
                let (cases, fallback) =
                    self.compile_int_cases(rows, branch_var);

                Decision::Switch(branch_var, cases, Some(fallback))
            }
            Type::String => {
                let (cases, fallback) =
                    self.compile_string_cases(rows, branch_var);

                Decision::Switch(branch_var, cases, Some(fallback))
            }
            Type::Finite(cases) => Decision::Switch(
                branch_var,
                self.compile_constructor_cases(rows, branch_var, cases),
                None,
            ),
        }
    }

    fn compile_int_cases(
        &mut self,
        rows: Vec<Row>,
        branch_var: Variable,
    ) -> (Vec<Case>, Box<Decision>) {
        let mut raw_cases: Vec<(Constructor, Vec<Variable>, Vec<Row>)> =
            Vec::new();
        let mut fallback_rows = Vec::new();
        let mut indexes: HashMap<(i64, i64), usize> = HashMap::new();

        for mut row in rows {
            let col = if let Some(col) = row.remove_column(&branch_var) {
                col
            } else {
                fallback_rows.push(row);
                continue;
            };

            let (key, cons) = match col.pattern {
                Pattern::Int(val) => ((val, val), Constructor::Int(val)),
                _ => unreachable!(),
            };

            if let Some(&index) = indexes.get(&key) {
                raw_cases[index].2.push(row);
                continue;
            }

            indexes.insert(key, raw_cases.len());
            raw_cases.push((cons, Vec::new(), vec![row]));
        }

        self.compile_literal_cases(raw_cases, fallback_rows)
    }

    fn compile_string_cases(
        &mut self,
        rows: Vec<Row>,
        branch_var: Variable,
    ) -> (Vec<Case>, Box<Decision>) {
        let mut raw_cases: Vec<(Constructor, Vec<Variable>, Vec<Row>)> =
            Vec::new();
        let mut fallback_rows = Vec::new();
        let mut indexes: HashMap<String, usize> = HashMap::new();

        for mut row in rows {
            let col = if let Some(col) = row.remove_column(&branch_var) {
                col
            } else {
                fallback_rows.push(row);
                continue;
            };

            let key = match col.pattern {
                Pattern::String(val) => val,
                _ => unreachable!(),
            };

            if let Some(&index) = indexes.get(&key) {
                raw_cases[index].2.push(row);
                continue;
            }

            indexes.insert(key.clone(), raw_cases.len());
            raw_cases.push((Constructor::String(key), Vec::new(), vec![row]));
        }

        self.compile_literal_cases(raw_cases, fallback_rows)
    }

    /// Compiles the cases and sub cases for the constructor located at the
    /// column of the branching variable.
    ///
    /// What exactly this method does may be a bit hard to understand from the
    /// code, as there's simply quite a bit going on. Roughly speaking, it does
    /// the following:
    ///
    /// 1. It takes the column we're branching on (based on the branching
    ///    variable) and removes it from every row.
    /// 2. We add additional columns to this row, if the constructor takes any
    ///    arguments (which we'll handle in a nested match).
    /// 3. We turn the resulting list of rows into a list of cases, then compile
    ///    those into decision (sub) trees.
    ///
    /// If a row didn't include the branching variable, we simply copy that row
    /// into the list of rows for every constructor to test.
    ///
    /// For this to work, the `cases` variable must be prepared such that it has
    /// a triple for every constructor we need to handle. For an ADT with 10
    /// constructors, that means 10 triples. This is needed so this method can
    /// assign the correct sub matches to these constructors.
    fn compile_constructor_cases(
        &mut self,
        rows: Vec<Row>,
        branch_var: Variable,
        mut cases: Vec<(Constructor, Vec<Variable>, Vec<Row>)>,
    ) -> Vec<Case> {
        for mut row in rows {
            let col = if let Some(col) = row.remove_column(&branch_var) {
                col
            } else {
                for (_, _, rows) in &mut cases {
                    rows.push(row.clone());
                }

                continue;
            };

            if let Pattern::Constructor(cons, args) = col.pattern {
                let idx = cons.index(self.db());
                let mut cols = row.columns;
                let case = &mut cases[idx];

                for (var, pat) in case.1.iter().zip(args.into_iter()) {
                    cols.push(Column::new(*var, pat));
                }

                case.2.push(Row::new(cols, row.guard, row.body));
            }
        }

        cases
            .into_iter()
            .map(|(cons, vars, rows)| {
                Case::new(cons, vars, self.compile_rows(rows))
            })
            .collect()
    }

    fn compile_literal_cases(
        &mut self,
        mut raw: Vec<(Constructor, Vec<Variable>, Vec<Row>)>,
        fallback: Vec<Row>,
    ) -> (Vec<Case>, Box<Decision>) {
        for (_, _, rows) in &mut raw {
            rows.append(&mut fallback.clone());
        }

        let cases = raw
            .into_iter()
            .map(|(cons, vars, rows)| {
                Case::new(cons, vars, self.compile_rows(rows))
            })
            .collect();

        (cases, Box::new(self.compile_rows(fallback)))
    }

    /// Given a row, returns the variable in that row that's referred to the
    /// most across all rows.
    fn branch_variable(&self, rows: &[Row]) -> Variable {
        let mut counts = HashMap::new();

        for row in rows {
            for col in &row.columns {
                *counts.entry(&col.variable).or_insert(0_usize) += 1
            }
        }

        rows[0]
            .columns
            .iter()
            .map(|col| col.variable)
            .max_by_key(|var| counts[var])
            .unwrap()
    }

    fn new_variables(
        &mut self,
        instance: ClassInstance,
        source_variable_type: TypeRef,
        types: Vec<TypeRef>,
    ) -> Vec<Variable> {
        if !instance.instance_of().is_generic(self.db()) {
            return types
                .into_iter()
                .map(|t| {
                    self.new_variable(
                        t.cast_according_to(source_variable_type, self.db()),
                    )
                })
                .collect();
        }

        let args = TypeArguments::for_class(self.db_mut(), instance);

        types
            .into_iter()
            .map(|raw_type| {
                let inferred =
                    TypeResolver::new(&mut self.state.db, &args, &self.bounds)
                        .resolve(raw_type)
                        .cast_according_to(source_variable_type, self.db());

                self.new_variable(inferred)
            })
            .collect()
    }

    fn variable_type(&mut self, variable: &Variable) -> Type {
        let typ = variable.value_type(&self.variables);
        let type_id = typ.type_id(self.db()).unwrap();
        let class_ins = if let TypeId::ClassInstance(ins) = type_id {
            ins
        } else {
            unreachable!()
        };
        let class_id = class_ins.instance_of();

        match class_id.0 {
            INT_ID => Type::Int,
            STRING_ID => Type::String,
            BOOL_ID => Type::Finite(vec![
                (Constructor::False, Vec::new(), Vec::new()),
                (Constructor::True, Vec::new(), Vec::new()),
            ]),
            _ => match class_id.kind(self.db()) {
                ClassKind::Enum => {
                    let cons = class_id
                        .variants(self.db())
                        .into_iter()
                        .map(|variant| {
                            let members = variant.members(self.db());

                            (
                                Constructor::Variant(variant),
                                self.new_variables(class_ins, typ, members),
                                Vec::new(),
                            )
                        })
                        .collect();

                    Type::Finite(cons)
                }
                ClassKind::Regular | ClassKind::Extern => {
                    let fields = class_id.fields(self.db());
                    let args = fields
                        .iter()
                        .map(|f| f.value_type(self.db()))
                        .collect();

                    Type::Finite(vec![(
                        Constructor::Class(fields),
                        self.new_variables(class_ins, typ, args),
                        Vec::new(),
                    )])
                }
                ClassKind::Tuple => {
                    let fields = class_id.fields(self.db());
                    let args = fields
                        .iter()
                        .map(|f| f.value_type(self.db()))
                        .collect();

                    Type::Finite(vec![(
                        Constructor::Tuple(fields),
                        self.new_variables(class_ins, typ, args),
                        Vec::new(),
                    )])
                }
                _ => unreachable!(),
            },
        }
    }

    fn db(&self) -> &Database {
        &self.state.db
    }

    fn db_mut(&mut self) -> &mut Database {
        &mut self.state.db
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use ast::source_location::SourceLocation;
    use similar_asserts::assert_eq;
    use types::module_name::ModuleName;
    use types::{
        Class, ClassInstance, ClassKind, Location, Module, TypeId,
        Variable as VariableType, VariableLocation, Visibility,
    };

    fn expr(value: i64) -> hir::Expression {
        hir::Expression::Int(Box::new(hir::IntLiteral {
            resolved_type: types::TypeRef::Unknown,
            value,
            location: SourceLocation::new(1..=1, 1..=1),
        }))
    }

    fn state() -> State {
        State::new(Config::new())
    }

    fn rules(input: Variable, patterns: Vec<(Pattern, BlockId)>) -> Vec<Row> {
        patterns
            .into_iter()
            .map(|(pat, block)| {
                Row::new(vec![Column::new(input, pat)], None, Body::new(block))
            })
            .collect()
    }

    fn rules_with_guard(
        input: Variable,
        patterns: Vec<(Pattern, Option<hir::Expression>, BlockId)>,
    ) -> Vec<Row> {
        patterns
            .into_iter()
            .map(|(pat, guard, block)| {
                Row::new(vec![Column::new(input, pat)], guard, Body::new(block))
            })
            .collect()
    }

    fn compiler(state: &mut State) -> Compiler {
        Compiler::new(state, Variables::new(), TypeBounds::new())
    }

    fn success(block: BlockId) -> Decision {
        Decision::Success(Body::new(block))
    }

    fn guard(
        code: hir::Expression,
        body: BlockId,
        fallback: Decision,
    ) -> Decision {
        Decision::Guard(code, Body::new(body), Box::new(fallback))
    }

    fn guard_with_bindings(
        code: hir::Expression,
        bindings: Vec<Binding>,
        body: BlockId,
        fallback: Decision,
    ) -> Decision {
        Decision::Guard(
            code,
            Body { bindings, block_id: body },
            Box::new(fallback),
        )
    }

    fn success_with_bindings(
        bindings: Vec<Binding>,
        block_id: BlockId,
    ) -> Decision {
        Decision::Success(Body { bindings, block_id })
    }

    fn fail() -> Decision {
        Decision::Fail
    }

    #[test]
    fn test_empty_rules() {
        let mut state = state();
        let compiler = compiler(&mut state);
        let result = compiler.compile(Vec::new());

        assert_eq!(result.tree, Decision::Fail);
    }

    #[test]
    fn test_nonexhaustive_int() {
        let mut state = state();
        let mut compiler = compiler(&mut state);
        let input = compiler.new_variable(TypeRef::int());
        let result = compiler.compile(rules(
            input,
            vec![(Pattern::Int(4), BlockId(1)), (Pattern::Int(5), BlockId(2))],
        ));

        assert_eq!(
            result.tree,
            Decision::Switch(
                input,
                vec![
                    Case::new(
                        Constructor::Int(4),
                        Vec::new(),
                        success(BlockId(1))
                    ),
                    Case::new(
                        Constructor::Int(5),
                        Vec::new(),
                        success(BlockId(2))
                    )
                ],
                Some(Box::new(fail()))
            )
        );
        assert!(result.missing);
        assert_eq!(result.missing_patterns(&state.db), vec!["_".to_string()]);
    }

    #[test]
    fn test_exhaustive_int() {
        let mut state = state();
        let mut compiler = compiler(&mut state);
        let input = compiler.new_variable(TypeRef::int());
        let result = compiler.compile(rules(
            input,
            vec![
                (Pattern::Int(4), BlockId(1)),
                (Pattern::Wildcard, BlockId(2)),
            ],
        ));

        assert_eq!(
            result.tree,
            Decision::Switch(
                input,
                vec![Case::new(
                    Constructor::Int(4),
                    Vec::new(),
                    success(BlockId(1))
                )],
                Some(Box::new(success_with_bindings(
                    vec![Binding::Ignored(input)],
                    BlockId(2)
                )))
            )
        );
    }

    #[test]
    fn test_nonexhaustive_string() {
        let mut state = state();
        let mut compiler = compiler(&mut state);
        let input = compiler.new_variable(TypeRef::string());
        let result = compiler.compile(rules(
            input,
            vec![(Pattern::String("a".to_string()), BlockId(1))],
        ));

        assert_eq!(
            result.tree,
            Decision::Switch(
                input,
                vec![Case::new(
                    Constructor::String("a".to_string()),
                    Vec::new(),
                    success(BlockId(1))
                )],
                Some(Box::new(fail()))
            )
        );
    }

    #[test]
    fn test_exhaustive_string() {
        let mut state = state();
        let mut compiler = compiler(&mut state);
        let input = compiler.new_variable(TypeRef::string());
        let result = compiler.compile(rules(
            input,
            vec![
                (Pattern::String("a".to_string()), BlockId(1)),
                (Pattern::Wildcard, BlockId(2)),
            ],
        ));

        assert_eq!(
            result.tree,
            Decision::Switch(
                input,
                vec![Case::new(
                    Constructor::String("a".to_string()),
                    Vec::new(),
                    success(BlockId(1))
                )],
                Some(Box::new(success_with_bindings(
                    vec![Binding::Ignored(input)],
                    BlockId(2)
                )))
            )
        );
    }

    #[test]
    fn test_or() {
        let mut state = state();
        let mut compiler = compiler(&mut state);
        let input = compiler.new_variable(TypeRef::int());
        let result = compiler.compile(rules(
            input,
            vec![(
                Pattern::Or(vec![Pattern::Int(4), Pattern::Int(5)]),
                BlockId(1),
            )],
        ));

        assert_eq!(
            result.tree,
            Decision::Switch(
                input,
                vec![
                    Case::new(
                        Constructor::Int(4),
                        Vec::new(),
                        success(BlockId(1))
                    ),
                    Case::new(
                        Constructor::Int(5),
                        Vec::new(),
                        success(BlockId(1))
                    )
                ],
                Some(Box::new(fail()))
            )
        );
    }

    #[test]
    fn test_wildcard() {
        let mut state = state();
        let mut compiler = compiler(&mut state);
        let input = compiler.new_variable(TypeRef::int());
        let result = compiler
            .compile(rules(input, vec![(Pattern::Wildcard, BlockId(1))]));

        assert_eq!(
            result.tree,
            success_with_bindings(vec![Binding::Ignored(input)], BlockId(1))
        );
    }

    #[test]
    fn test_variable() {
        let mut state = state();
        let loc = VariableLocation::new(1, 1, 1);
        let bind = VariableType::alloc(
            &mut state.db,
            "a".to_string(),
            TypeRef::int(),
            false,
            loc,
        );
        let mut compiler = compiler(&mut state);
        let input = compiler.new_variable(TypeRef::int());
        let result = compiler
            .compile(rules(input, vec![(Pattern::Variable(bind), BlockId(1))]));

        assert_eq!(
            result.tree,
            success_with_bindings(
                vec![Binding::Named(bind, input)],
                BlockId(1)
            )
        );
    }

    #[test]
    fn test_nonexhaustive_constructor() {
        let mut state = state();
        let module = Module::alloc(
            &mut state.db,
            ModuleName::new("test"),
            "test.inko".into(),
        );
        let option_type = Class::alloc(
            &mut state.db,
            "Option".to_string(),
            ClassKind::Enum,
            Visibility::Public,
            module,
            Location::default(),
        );
        let some = option_type.new_variant(
            &mut state.db,
            "Some".to_string(),
            vec![TypeRef::int()],
            Location::default(),
        );
        let none = option_type.new_variant(
            &mut state.db,
            "None".to_string(),
            Vec::new(),
            Location::default(),
        );
        let mut compiler = compiler(&mut state);
        let input = compiler.new_variable(TypeRef::Owned(
            TypeId::ClassInstance(ClassInstance::new(option_type)),
        ));
        let int_var = Variable(1);
        let result = compiler.compile(rules(
            input,
            vec![(
                Pattern::Constructor(
                    Constructor::Variant(some),
                    vec![Pattern::Int(4)],
                ),
                BlockId(1),
            )],
        ));

        assert_eq!(
            result.tree,
            Decision::Switch(
                input,
                vec![
                    Case::new(
                        Constructor::Variant(some),
                        vec![int_var],
                        Decision::Switch(
                            int_var,
                            vec![Case::new(
                                Constructor::Int(4),
                                Vec::new(),
                                success(BlockId(1))
                            )],
                            Some(Box::new(fail()))
                        ),
                    ),
                    Case::new(Constructor::Variant(none), Vec::new(), fail())
                ],
                None
            )
        );
    }

    #[test]
    fn test_exhaustive_constructor() {
        let mut state = state();
        let module = Module::alloc(
            &mut state.db,
            ModuleName::new("test"),
            "test.inko".into(),
        );
        let option_type = Class::alloc(
            &mut state.db,
            "Option".to_string(),
            ClassKind::Enum,
            Visibility::Public,
            module,
            Location::default(),
        );
        let some = option_type.new_variant(
            &mut state.db,
            "Some".to_string(),
            vec![TypeRef::int()],
            Location::default(),
        );
        let none = option_type.new_variant(
            &mut state.db,
            "None".to_string(),
            Vec::new(),
            Location::default(),
        );
        let mut compiler = compiler(&mut state);
        let input = compiler.new_variable(TypeRef::Owned(
            TypeId::ClassInstance(ClassInstance::new(option_type)),
        ));
        let int_var = Variable(1);
        let result = compiler.compile(rules(
            input,
            vec![
                (
                    Pattern::Constructor(
                        Constructor::Variant(some),
                        vec![Pattern::Wildcard],
                    ),
                    BlockId(1),
                ),
                (
                    Pattern::Constructor(
                        Constructor::Variant(none),
                        Vec::new(),
                    ),
                    BlockId(2),
                ),
            ],
        ));

        assert_eq!(
            result.tree,
            Decision::Switch(
                input,
                vec![
                    Case::new(
                        Constructor::Variant(some),
                        vec![int_var],
                        success_with_bindings(
                            vec![Binding::Ignored(int_var)],
                            BlockId(1)
                        ),
                    ),
                    Case::new(
                        Constructor::Variant(none),
                        Vec::new(),
                        success(BlockId(2))
                    )
                ],
                None
            )
        );
    }

    #[test]
    fn test_nonexhaustive_class() {
        let mut state = state();
        let module = Module::alloc(
            &mut state.db,
            ModuleName::new("test"),
            "test.inko".into(),
        );
        let person_type = Class::alloc(
            &mut state.db,
            "Person".to_string(),
            ClassKind::Regular,
            Visibility::Public,
            module,
            Location::default(),
        );

        person_type.new_field(
            &mut state.db,
            "name".to_string(),
            0,
            TypeRef::string(),
            Visibility::Public,
            module,
            Location::default(),
        );

        person_type.new_field(
            &mut state.db,
            "age".to_string(),
            1,
            TypeRef::int(),
            Visibility::Public,
            module,
            Location::default(),
        );

        let fields = person_type.fields(&state.db);
        let mut compiler = compiler(&mut state);
        let input = compiler.new_variable(TypeRef::Owned(
            TypeId::ClassInstance(ClassInstance::new(person_type)),
        ));
        let name_var = Variable(1);
        let age_var = Variable(2);
        let result = compiler.compile(rules(
            input,
            vec![(
                Pattern::Constructor(
                    Constructor::Class(fields.clone()),
                    vec![Pattern::String("Alice".to_string()), Pattern::Int(4)],
                ),
                BlockId(1),
            )],
        ));

        assert_eq!(
            result.tree,
            Decision::Switch(
                input,
                vec![Case::new(
                    Constructor::Class(fields),
                    vec![name_var, age_var],
                    Decision::Switch(
                        age_var,
                        vec![Case::new(
                            Constructor::Int(4),
                            Vec::new(),
                            Decision::Switch(
                                name_var,
                                vec![Case::new(
                                    Constructor::String("Alice".to_string()),
                                    Vec::new(),
                                    success(BlockId(1))
                                )],
                                Some(Box::new(fail()))
                            ),
                        )],
                        Some(Box::new(fail()))
                    ),
                )],
                None
            )
        );
        assert!(result.missing);
        assert_eq!(result.missing_patterns(&state.db), vec!["_".to_string()]);
    }

    #[test]
    fn test_nonexhaustive_tuple() {
        let mut state = state();
        let module = Module::alloc(
            &mut state.db,
            ModuleName::new("test"),
            "test.inko".into(),
        );
        let tuple2 = Class::alloc(
            &mut state.db,
            "Tuple2".to_string(),
            ClassKind::Tuple,
            Visibility::Public,
            module,
            Location::default(),
        );

        tuple2.new_field(
            &mut state.db,
            "0".to_string(),
            0,
            TypeRef::string(),
            Visibility::Public,
            module,
            Location::default(),
        );

        tuple2.new_field(
            &mut state.db,
            "1".to_string(),
            1,
            TypeRef::int(),
            Visibility::Public,
            module,
            Location::default(),
        );

        let tuple_fields = tuple2.fields(&state.db);
        let mut compiler = compiler(&mut state);
        let input = compiler.new_variable(TypeRef::Owned(
            TypeId::ClassInstance(ClassInstance::new(tuple2)),
        ));
        let var1 = Variable(1);
        let var2 = Variable(2);
        let result = compiler.compile(rules(
            input,
            vec![(
                Pattern::Constructor(
                    Constructor::Tuple(tuple_fields.clone()),
                    vec![Pattern::String("Alice".to_string()), Pattern::Int(4)],
                ),
                BlockId(1),
            )],
        ));

        assert_eq!(
            result.tree,
            Decision::Switch(
                input,
                vec![Case::new(
                    Constructor::Tuple(tuple_fields),
                    vec![var1, var2],
                    Decision::Switch(
                        var2,
                        vec![Case::new(
                            Constructor::Int(4),
                            Vec::new(),
                            Decision::Switch(
                                var1,
                                vec![Case::new(
                                    Constructor::String("Alice".to_string()),
                                    Vec::new(),
                                    success(BlockId(1))
                                )],
                                Some(Box::new(fail()))
                            ),
                        )],
                        Some(Box::new(fail()))
                    ),
                )],
                None
            )
        );
        assert!(result.missing);
        assert_eq!(
            result.missing_patterns(&state.db),
            vec!["(_, _)".to_string()]
        );
    }

    #[test]
    fn test_exhaustive_class() {
        let mut state = state();
        let module = Module::alloc(
            &mut state.db,
            ModuleName::new("test"),
            "test.inko".into(),
        );
        let person_type = Class::alloc(
            &mut state.db,
            "Person".to_string(),
            ClassKind::Regular,
            Visibility::Public,
            module,
            Location::default(),
        );

        person_type.new_field(
            &mut state.db,
            "name".to_string(),
            0,
            TypeRef::string(),
            Visibility::Public,
            module,
            Location::default(),
        );

        person_type.new_field(
            &mut state.db,
            "age".to_string(),
            1,
            TypeRef::int(),
            Visibility::Public,
            module,
            Location::default(),
        );

        let fields = person_type.fields(&state.db);
        let mut compiler = compiler(&mut state);
        let input = compiler.new_variable(TypeRef::Owned(
            TypeId::ClassInstance(ClassInstance::new(person_type)),
        ));
        let name_var = Variable(1);
        let age_var = Variable(2);
        let result = compiler.compile(rules(
            input,
            vec![(
                Pattern::Constructor(
                    Constructor::Class(fields.clone()),
                    vec![Pattern::Wildcard, Pattern::Wildcard],
                ),
                BlockId(1),
            )],
        ));

        assert_eq!(
            result.tree,
            Decision::Switch(
                input,
                vec![Case::new(
                    Constructor::Class(fields),
                    vec![name_var, age_var],
                    success_with_bindings(
                        vec![
                            Binding::Ignored(name_var),
                            Binding::Ignored(age_var)
                        ],
                        BlockId(1)
                    )
                )],
                None
            )
        );
    }

    #[test]
    fn test_exhaustive_tuple() {
        let mut state = state();
        let module = Module::alloc(
            &mut state.db,
            ModuleName::new("test"),
            "test.inko".into(),
        );
        let tuple2 = Class::alloc(
            &mut state.db,
            "Tuple2".to_string(),
            ClassKind::Tuple,
            Visibility::Public,
            module,
            Location::default(),
        );

        tuple2.new_field(
            &mut state.db,
            "0".to_string(),
            0,
            TypeRef::string(),
            Visibility::Public,
            module,
            Location::default(),
        );

        tuple2.new_field(
            &mut state.db,
            "1".to_string(),
            1,
            TypeRef::int(),
            Visibility::Public,
            module,
            Location::default(),
        );

        let tuple_fields = tuple2.fields(&state.db);
        let mut compiler = compiler(&mut state);
        let input = compiler.new_variable(TypeRef::Owned(
            TypeId::ClassInstance(ClassInstance::new(tuple2)),
        ));
        let var1 = Variable(1);
        let var2 = Variable(2);
        let result = compiler.compile(rules(
            input,
            vec![(
                Pattern::Constructor(
                    Constructor::Tuple(tuple_fields.clone()),
                    vec![Pattern::Wildcard, Pattern::Wildcard],
                ),
                BlockId(1),
            )],
        ));

        assert_eq!(
            result.tree,
            Decision::Switch(
                input,
                vec![Case::new(
                    Constructor::Tuple(tuple_fields),
                    vec![var1, var2],
                    success_with_bindings(
                        vec![Binding::Ignored(var1), Binding::Ignored(var2)],
                        BlockId(1)
                    ),
                )],
                None
            )
        );
    }

    #[test]
    fn test_nonexhaustive_guard() {
        let mut state = state();
        let mut compiler = compiler(&mut state);
        let input = compiler.new_variable(TypeRef::int());
        let result = compiler.compile(rules_with_guard(
            input,
            vec![
                (Pattern::Int(4), None, BlockId(1)),
                (Pattern::Wildcard, Some(expr(3)), BlockId(2)),
            ],
        ));

        assert_eq!(
            result.tree,
            Decision::Switch(
                input,
                vec![Case::new(
                    Constructor::Int(4),
                    Vec::new(),
                    success(BlockId(1))
                )],
                Some(Box::new(guard_with_bindings(
                    expr(3),
                    vec![Binding::Ignored(input)],
                    BlockId(2),
                    fail()
                )))
            )
        );
        assert!(result.missing);
        assert_eq!(result.missing_patterns(&state.db), vec!["_".to_string()]);
    }

    #[test]
    fn test_exhaustive_guard() {
        let mut state = state();
        let mut compiler = compiler(&mut state);
        let input = compiler.new_variable(TypeRef::int());
        let result = compiler.compile(rules_with_guard(
            input,
            vec![
                (Pattern::Int(4), Some(expr(3)), BlockId(1)),
                (Pattern::Wildcard, None, BlockId(2)),
            ],
        ));

        assert_eq!(
            result.tree,
            Decision::Switch(
                input,
                vec![Case::new(
                    Constructor::Int(4),
                    Vec::new(),
                    guard(
                        expr(3),
                        BlockId(1),
                        success_with_bindings(
                            vec![Binding::Ignored(input)],
                            BlockId(2)
                        )
                    )
                )],
                Some(Box::new(success_with_bindings(
                    vec![Binding::Ignored(input)],
                    BlockId(2)
                )))
            )
        );
    }

    #[test]
    fn test_guard_with_or_pattern() {
        let mut state = state();
        let mut compiler = compiler(&mut state);
        let input = compiler.new_variable(TypeRef::int());
        let result = compiler.compile(rules_with_guard(
            input,
            vec![
                (
                    Pattern::Or(vec![Pattern::Int(4), Pattern::Int(5)]),
                    Some(expr(42)),
                    BlockId(1),
                ),
                (Pattern::Int(4), None, BlockId(2)),
                (Pattern::Int(5), None, BlockId(3)),
                (Pattern::Wildcard, None, BlockId(4)),
            ],
        ));

        assert_eq!(
            result.tree,
            Decision::Switch(
                input,
                vec![
                    Case::new(
                        Constructor::Int(4),
                        Vec::new(),
                        guard(expr(42), BlockId(1), success(BlockId(2)))
                    ),
                    Case::new(
                        Constructor::Int(5),
                        Vec::new(),
                        guard(expr(42), BlockId(1), success(BlockId(3)))
                    )
                ],
                Some(Box::new(success_with_bindings(
                    vec![Binding::Ignored(input)],
                    BlockId(4)
                )))
            )
        );
    }

    #[test]
    fn test_guard_with_same_int() {
        let mut state = state();
        let mut compiler = compiler(&mut state);
        let input = compiler.new_variable(TypeRef::int());
        let result = compiler.compile(rules_with_guard(
            input,
            vec![
                (Pattern::Int(4), Some(expr(10)), BlockId(1)),
                (Pattern::Int(4), Some(expr(20)), BlockId(2)),
                (Pattern::Wildcard, None, BlockId(3)),
            ],
        ));

        assert_eq!(
            result.tree,
            Decision::Switch(
                input,
                vec![Case::new(
                    Constructor::Int(4),
                    Vec::new(),
                    guard(
                        expr(10),
                        BlockId(1),
                        guard(
                            expr(20),
                            BlockId(2),
                            success_with_bindings(
                                vec![Binding::Ignored(input)],
                                BlockId(3)
                            )
                        )
                    )
                )],
                Some(Box::new(success_with_bindings(
                    vec![Binding::Ignored(input)],
                    BlockId(3)
                )))
            )
        );
    }

    #[test]
    fn test_guard_with_same_string() {
        let mut state = state();
        let mut compiler = compiler(&mut state);
        let input = compiler.new_variable(TypeRef::string());
        let result = compiler.compile(rules_with_guard(
            input,
            vec![
                (Pattern::String("a".to_string()), Some(expr(3)), BlockId(1)),
                (Pattern::String("a".to_string()), Some(expr(4)), BlockId(2)),
                (Pattern::Wildcard, None, BlockId(3)),
            ],
        ));

        assert_eq!(
            result.tree,
            Decision::Switch(
                input,
                vec![Case::new(
                    Constructor::String("a".to_string()),
                    Vec::new(),
                    guard(
                        expr(3),
                        BlockId(1),
                        guard(
                            expr(4),
                            BlockId(2),
                            success_with_bindings(
                                vec![Binding::Ignored(input)],
                                BlockId(3)
                            )
                        )
                    )
                )],
                Some(Box::new(success_with_bindings(
                    vec![Binding::Ignored(input)],
                    BlockId(3)
                )))
            )
        );
    }

    #[test]
    fn test_redundant_pattern() {
        let mut state = state();
        let mut compiler = compiler(&mut state);
        let input = compiler.new_variable(TypeRef::int());
        let result = compiler.compile(rules(
            input,
            vec![
                (Pattern::Int(4), BlockId(1)),
                (Pattern::Int(4), BlockId(2)),
                (Pattern::Wildcard, BlockId(3)),
            ],
        ));

        assert_eq!(
            result.tree,
            Decision::Switch(
                input,
                vec![Case::new(
                    Constructor::Int(4),
                    Vec::new(),
                    success(BlockId(1))
                )],
                Some(Box::new(success_with_bindings(
                    vec![Binding::Ignored(input)],
                    BlockId(3)
                )))
            )
        );
    }

    #[test]
    fn test_exhaustive_nested_int() {
        let mut state = state();
        let module = Module::alloc(
            &mut state.db,
            ModuleName::new("test"),
            "test.inko".into(),
        );
        let tuple2 = Class::alloc(
            &mut state.db,
            "Tuple2".to_string(),
            ClassKind::Tuple,
            Visibility::Public,
            module,
            Location::default(),
        );

        tuple2.new_field(
            &mut state.db,
            "0".to_string(),
            0,
            TypeRef::int(),
            Visibility::Public,
            module,
            Location::default(),
        );

        tuple2.new_field(
            &mut state.db,
            "1".to_string(),
            1,
            TypeRef::string(),
            Visibility::Public,
            module,
            Location::default(),
        );

        let tuple_fields = tuple2.fields(&state.db);
        let mut compiler = compiler(&mut state);
        let input = compiler.new_variable(TypeRef::Owned(
            TypeId::ClassInstance(ClassInstance::new(tuple2)),
        ));
        let var1 = Variable(1);
        let var2 = Variable(2);
        let result = compiler.compile(rules(
            input,
            vec![
                (
                    Pattern::Constructor(
                        Constructor::Tuple(tuple_fields.clone()),
                        vec![Pattern::Int(4), Pattern::String("a".to_string())],
                    ),
                    BlockId(1),
                ),
                (
                    Pattern::Constructor(
                        Constructor::Tuple(tuple_fields.clone()),
                        vec![Pattern::Wildcard, Pattern::Wildcard],
                    ),
                    BlockId(2),
                ),
            ],
        ));

        assert_eq!(
            result.tree,
            Decision::Switch(
                input,
                vec![Case::new(
                    Constructor::Tuple(tuple_fields),
                    vec![var1, var2],
                    Decision::Switch(
                        var2,
                        vec![Case::new(
                            Constructor::String("a".to_string()),
                            Vec::new(),
                            Decision::Switch(
                                var1,
                                vec![Case::new(
                                    Constructor::Int(4),
                                    Vec::new(),
                                    success(BlockId(1))
                                )],
                                Some(Box::new(success_with_bindings(
                                    vec![
                                        Binding::Ignored(var1),
                                        Binding::Ignored(var2)
                                    ],
                                    BlockId(2)
                                )))
                            )
                        )],
                        Some(Box::new(success_with_bindings(
                            vec![
                                Binding::Ignored(var1),
                                Binding::Ignored(var2)
                            ],
                            BlockId(2)
                        )))
                    ),
                )],
                None
            )
        );
    }

    #[test]
    fn test_exhaustive_nested_bool() {
        let mut state = state();
        let module = Module::alloc(
            &mut state.db,
            ModuleName::new("test"),
            "test.inko".into(),
        );
        let tuple2 = Class::alloc(
            &mut state.db,
            "Tuple2".to_string(),
            ClassKind::Tuple,
            Visibility::Public,
            module,
            Location::default(),
        );

        tuple2.new_field(
            &mut state.db,
            "0".to_string(),
            0,
            TypeRef::boolean(),
            Visibility::Public,
            module,
            Location::default(),
        );

        tuple2.new_field(
            &mut state.db,
            "1".to_string(),
            1,
            TypeRef::string(),
            Visibility::Public,
            module,
            Location::default(),
        );

        let tuple_fields = tuple2.fields(&state.db);
        let mut compiler = compiler(&mut state);
        let input = compiler.new_variable(TypeRef::Owned(
            TypeId::ClassInstance(ClassInstance::new(tuple2)),
        ));
        let var1 = Variable(1);
        let var2 = Variable(2);
        let result = compiler.compile(rules(
            input,
            vec![
                (
                    Pattern::Constructor(
                        Constructor::Tuple(tuple_fields.clone()),
                        vec![
                            Pattern::Constructor(Constructor::True, Vec::new()),
                            Pattern::String("a".to_string()),
                        ],
                    ),
                    BlockId(1),
                ),
                (
                    Pattern::Constructor(
                        Constructor::Tuple(tuple_fields.clone()),
                        vec![Pattern::Wildcard, Pattern::Wildcard],
                    ),
                    BlockId(2),
                ),
            ],
        ));

        assert_eq!(
            result.tree,
            Decision::Switch(
                input,
                vec![Case::new(
                    Constructor::Tuple(tuple_fields),
                    vec![var1, var2],
                    Decision::Switch(
                        var2,
                        vec![Case::new(
                            Constructor::String("a".to_string()),
                            Vec::new(),
                            Decision::Switch(
                                var1,
                                vec![
                                    Case::new(
                                        Constructor::False,
                                        Vec::new(),
                                        success_with_bindings(
                                            vec![
                                                Binding::Ignored(var1),
                                                Binding::Ignored(var2)
                                            ],
                                            BlockId(2)
                                        )
                                    ),
                                    Case::new(
                                        Constructor::True,
                                        Vec::new(),
                                        success(BlockId(1))
                                    )
                                ],
                                None
                            )
                        )],
                        Some(Box::new(success_with_bindings(
                            vec![
                                Binding::Ignored(var1),
                                Binding::Ignored(var2)
                            ],
                            BlockId(2)
                        )))
                    ),
                )],
                None
            )
        );
    }

    #[test]
    fn test_tuple_with_or_and_bindings() {
        let mut state = state();
        let module = Module::alloc(
            &mut state.db,
            ModuleName::new("test"),
            "test.inko".into(),
        );
        let tuple2 = Class::alloc(
            &mut state.db,
            "Tuple2".to_string(),
            ClassKind::Tuple,
            Visibility::Public,
            module,
            Location::default(),
        );

        tuple2.new_field(
            &mut state.db,
            "0".to_string(),
            0,
            TypeRef::int(),
            Visibility::Public,
            module,
            Location::default(),
        );

        tuple2.new_field(
            &mut state.db,
            "1".to_string(),
            1,
            TypeRef::int(),
            Visibility::Public,
            module,
            Location::default(),
        );

        let tuple_fields = tuple2.fields(&state.db);
        let mut compiler = compiler(&mut state);
        let input = compiler.new_variable(TypeRef::Owned(
            TypeId::ClassInstance(ClassInstance::new(tuple2)),
        ));
        let var1 = Variable(1);
        let var2 = Variable(2);
        let result = compiler.compile(rules(
            input,
            vec![(
                Pattern::Constructor(
                    Constructor::Tuple(tuple_fields.clone()),
                    vec![
                        Pattern::Variable(VariableId(0)),
                        Pattern::Or(vec![
                            Pattern::Variable(VariableId(1)),
                            Pattern::Variable(VariableId(2)),
                        ]),
                    ],
                ),
                BlockId(1),
            )],
        ));

        assert_eq!(
            result.tree,
            Decision::Switch(
                input,
                vec![Case::new(
                    Constructor::Tuple(tuple_fields),
                    vec![var1, var2],
                    success_with_bindings(
                        vec![
                            Binding::Named(VariableId(0), var1),
                            Binding::Named(VariableId(1), var2)
                        ],
                        BlockId(1)
                    ),
                )],
                None
            )
        );
    }
}
