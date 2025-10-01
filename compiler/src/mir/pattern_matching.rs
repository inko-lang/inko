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
    ConstructorId, Database, FieldId, TypeArguments, TypeBounds, TypeEnum,
    TypeInstance, TypeKind, TypeRef, VariableId, ARRAY_ID, BOOL_ID,
    BYTES_MODULE, BYTE_ARRAY_TYPE, INT_ID, SLICE_TYPE, STRING_ID,
};

fn add_constructor_pattern(
    db: &Database,
    var: &Variable,
    case: &Case,
    terms: &mut Vec<Term>,
) {
    match &case.constructor {
        Constructor::True => {
            let name = "true".to_string();

            terms.push(Term::new(*var, name, Vec::new()));
        }
        Constructor::False => {
            let name = "false".to_string();

            terms.push(Term::new(*var, name, Vec::new()));
        }
        Constructor::Int(_)
        | Constructor::String(_)
        | Constructor::Array(_) => {
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
        Constructor::Constructor(constructor) => {
            let args = case.arguments.clone();
            let name = constructor.name(db).clone();

            terms.push(Term::new(*var, name, args));
        }
    }
}

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
                add_constructor_pattern(db, var, case, terms);
                add_missing_patterns(db, &case.node, terms, missing);
                terms.pop();
            }

            if let Some(node) = fallback {
                add_missing_patterns(db, node, terms, missing);
            }
        }
        Decision::SwitchArray(var, cases, fallback) => {
            for case in cases {
                add_constructor_pattern(db, var, case, terms);
                add_missing_patterns(db, &case.node, terms, missing);
                terms.pop();
            }

            add_missing_patterns(db, fallback, terms, missing);
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
/// translate to the `Type::Finite` constructor.
enum Type {
    Int,
    String,

    /// An array of some type T.
    Array(TypeRef),

    /// An enum.
    Enum(Vec<RawCase>),

    /// A regular type with finite constructors, such as a tuple.
    Regular(Vec<RawCase>),
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
    Constructor(ConstructorId),
    Array(usize),
}

impl Constructor {
    /// Returns the index of this constructor relative to its type.
    pub(crate) fn index(&self, db: &Database) -> usize {
        match self {
            Constructor::False
            | Constructor::Int(_)
            | Constructor::String(_)
            | Constructor::Class(_)
            | Constructor::Tuple(_)
            | Constructor::Array(_) => 0,
            Constructor::True => 1,
            Constructor::Constructor(id) => id.id(db) as usize,
        }
    }
}

/// A user defined pattern such as `Some((x, 10))`.
#[derive(Clone, Eq, PartialEq, Debug)]
pub(crate) enum Pattern {
    Constructor(Constructor, Vec<Pattern>),
    Int(i64),
    String(String),
    Array(Vec<Pattern>),
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
            hir::Pattern::Type(n) => {
                let len = n.type_id.unwrap().number_of_fields(db);
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
                types::ConstantPatternKind::Constructor(id) => {
                    Pattern::Constructor(
                        Constructor::Constructor(id),
                        Vec::new(),
                    )
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
            hir::Pattern::Array(n) => {
                let args = n
                    .values
                    .into_iter()
                    .map(|p| Pattern::from_hir(db, mir, p))
                    .collect();

                Pattern::Array(args)
            }
            hir::Pattern::Constructor(n) => {
                let args = n
                    .values
                    .into_iter()
                    .map(|p| Pattern::from_hir(db, mir, p))
                    .collect();

                Pattern::Constructor(
                    Constructor::Constructor(n.constructor_id.unwrap()),
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

struct RawCase {
    /// The constructor to test against an input variable.
    constructor: Constructor,

    /// Variables to introduce to the body of this case.
    arguments: Vec<Variable>,

    /// An array for storing rows to use for building the constructor's sub
    /// tree.
    rows: Vec<Row>,

    /// If the pattern for this case is explicitly given and thus visited.
    visited: bool,
}

impl RawCase {
    fn new(constructor: Constructor, arguments: Vec<Variable>) -> Self {
        Self { constructor, arguments, rows: Vec::new(), visited: false }
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
    /// 1. The guard to evaluate
    /// 2. The body to evaluate if the guard matches
    /// 3. The sub tree to evaluate when the guard fails
    Guard(hir::Expression, Body, Box<Decision>),

    /// Checks if a value is any of the given patterns.
    ///
    /// The arguments are as follows:
    ///
    /// 1. The variable to test
    /// 2. The cases to test against this variable
    /// 3. A fallback decision to take, in case none of the cases matched
    Switch(Variable, Vec<Case>, Option<Box<Decision>>),

    /// Branches on the size of an array and processes the corresponding cases.
    ///
    /// The arguments are as follows:
    ///
    /// 1. The variable to test
    /// 2. The cases to test against, each case should have a
    ///    `Constructor::Array` with the corresponding number of values as its
    ///    argument
    /// 3. A fallback decision to take for inputs of an unknown size
    SwitchArray(Variable, Vec<Case>, Box<Decision>),
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

#[derive(Eq, PartialEq, Hash)]
enum Key {
    Int(i64),
    String(String),
}

/// A type for compiling a HIR `match` expression into a decision tree.
pub(crate) struct Compiler<'a> {
    state: &'a mut State,

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
        Self { state, missing: false, variables, bounds }
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
        if rows.first().is_some_and(|c| c.columns.is_empty()) {
            let row = rows.remove(0);

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
            Type::Int | Type::String => {
                let (cases, fallback) =
                    self.compile_infinite_cases(rows, branch_var);

                Decision::Switch(branch_var, cases, Some(fallback))
            }
            Type::Array(t) => self.compile_array_cases(t, rows, branch_var),
            Type::Regular(cases) => Decision::Switch(
                branch_var,
                self.compile_regular_type_cases(rows, branch_var, cases),
                None,
            ),
            Type::Enum(cases) => {
                let (cases, fallback) =
                    self.compile_enum_cases(rows, branch_var, cases);

                Decision::Switch(branch_var, cases, fallback)
            }
        }
    }

    fn compile_infinite_cases(
        &mut self,
        rows: Vec<Row>,
        branch_var: Variable,
    ) -> (Vec<Case>, Box<Decision>) {
        let mut raw_cases: Vec<RawCase> = Vec::new();
        let mut fallback_rows = Vec::new();
        let mut indexes: HashMap<Key, usize> = HashMap::new();

        for mut row in rows {
            let col = if let Some(col) = row.remove_column(&branch_var) {
                col
            } else {
                for c in &mut raw_cases {
                    c.rows.push(row.clone());
                }

                fallback_rows.push(row);
                continue;
            };

            let (key, cons) = match col.pattern {
                Pattern::Int(v) => (Key::Int(v), Constructor::Int(v)),
                Pattern::String(v) => {
                    (Key::String(v.clone()), Constructor::String(v))
                }
                _ => unreachable!(),
            };

            if let Some(&index) = indexes.get(&key) {
                raw_cases[index].rows.push(row);
                continue;
            }

            let mut rows = fallback_rows.clone();

            rows.push(row);
            indexes.insert(key, raw_cases.len());
            raw_cases.push(RawCase {
                constructor: cons,
                arguments: Vec::new(),
                rows,
                visited: false,
            });
        }

        let cases = raw_cases
            .into_iter()
            .map(|raw| {
                Case::new(
                    raw.constructor,
                    raw.arguments,
                    self.compile_rows(raw.rows),
                )
            })
            .collect();

        (cases, Box::new(self.compile_rows(fallback_rows)))
    }

    /// Compiles the cases and sub cases for the enum constructor located at the
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
    fn compile_enum_cases(
        &mut self,
        rows: Vec<Row>,
        branch_var: Variable,
        mut cases: Vec<RawCase>,
    ) -> (Vec<Case>, Option<Box<Decision>>) {
        for mut row in rows {
            let col = if let Some(col) = row.remove_column(&branch_var) {
                col
            } else {
                cases.iter_mut().for_each(|c| c.rows.push(row.clone()));
                continue;
            };

            if let Pattern::Constructor(cons, args) = col.pattern {
                let idx = cons.index(self.db());
                let mut cols = row.columns;
                let case = &mut cases[idx];

                for (var, pat) in case.arguments.iter().zip(args.into_iter()) {
                    cols.push(Column::new(*var, pat));
                }

                case.rows.push(Row::new(cols, row.guard, row.body));
                case.visited = true;
            }
        }

        let mut res = Vec::new();
        let mut fallback = None;

        for raw in cases {
            if raw.visited {
                res.push(Case::new(
                    raw.constructor,
                    raw.arguments,
                    self.compile_rows(raw.rows),
                ));
            } else if fallback.is_none() {
                // For cases/patterns not visited the rows are always the same,
                // so we just pick the first one and use that as the fallback.
                fallback = Some(Box::new(self.compile_rows(raw.rows)));
            }
        }

        (res, fallback)
    }

    fn compile_regular_type_cases(
        &mut self,
        rows: Vec<Row>,
        branch_var: Variable,
        mut cases: Vec<RawCase>,
    ) -> Vec<Case> {
        for mut row in rows {
            let col = if let Some(col) = row.remove_column(&branch_var) {
                col
            } else {
                cases.iter_mut().for_each(|c| c.rows.push(row.clone()));
                continue;
            };

            if let Pattern::Constructor(cons, args) = col.pattern {
                let idx = cons.index(self.db());
                let mut cols = row.columns;
                let case = &mut cases[idx];

                for (var, pat) in case.arguments.iter().zip(args.into_iter()) {
                    cols.push(Column::new(*var, pat));
                }

                case.rows.push(Row::new(cols, row.guard, row.body));
            }
        }

        cases
            .into_iter()
            .map(|r| {
                Case::new(r.constructor, r.arguments, self.compile_rows(r.rows))
            })
            .collect()
    }

    fn compile_array_cases(
        &mut self,
        value_type: TypeRef,
        rows: Vec<Row>,
        branch_var: Variable,
    ) -> Decision {
        let mut raw_cases: Vec<(Constructor, Vec<Variable>, Vec<Row>)> =
            Vec::new();
        let mut fallback_rows = Vec::new();
        let mut indexes: HashMap<usize, usize> = HashMap::new();
        let mut vars = Vec::new();

        for mut row in rows {
            let col = if let Some(col) = row.remove_column(&branch_var) {
                col
            } else {
                for (_, _, rows) in &mut raw_cases {
                    rows.push(row.clone());
                }

                fallback_rows.push(row);
                continue;
            };

            let Pattern::Array(args) = col.pattern else { unreachable!() };

            if args.len() > vars.len() {
                for _ in 0..(args.len() - vars.len()) {
                    vars.push(self.new_variable(value_type));
                }
            }

            let cons = Constructor::Array(args.len());
            let key = args.len();
            let case = if let Some(&idx) = indexes.get(&key) {
                &mut raw_cases[idx]
            } else {
                let idx = raw_cases.len();
                let cvars = vars[0..args.len()].to_vec();

                raw_cases.push((cons, cvars, fallback_rows.clone()));
                indexes.insert(key, idx);
                &mut raw_cases[idx]
            };

            let mut cols = row.columns;

            for (var, pat) in case.1.iter().zip(args.into_iter()) {
                cols.push(Column::new(*var, pat));
            }

            case.2.push(Row::new(cols, row.guard, row.body));
        }

        let cases = raw_cases
            .into_iter()
            .map(|(c, v, r)| Case::new(c, v, self.compile_rows(r)))
            .collect();
        let fallback = Box::new(self.compile_rows(fallback_rows));

        Decision::SwitchArray(branch_var, cases, fallback)
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
        instance: TypeInstance,
        source_variable_type: TypeRef,
        types: Vec<TypeRef>,
    ) -> Vec<Variable> {
        if !instance.instance_of().is_generic(self.db()) {
            return types
                .into_iter()
                .map(|t| {
                    self.new_variable(
                        t.cast_according_to(self.db(), source_variable_type),
                    )
                })
                .collect();
        }

        let args = TypeArguments::for_type(self.db_mut(), instance);

        types
            .into_iter()
            .map(|raw_type| {
                let inferred =
                    TypeResolver::new(&mut self.state.db, &args, &self.bounds)
                        .resolve(raw_type)
                        .cast_according_to(self.db(), source_variable_type);

                self.new_variable(inferred)
            })
            .collect()
    }

    fn variable_type(&mut self, variable: &Variable) -> Type {
        let typ = variable.value_type(&self.variables);
        let type_id = typ.as_type_enum(self.db()).unwrap();
        let type_ins = if let TypeEnum::TypeInstance(ins) = type_id {
            ins
        } else {
            unreachable!()
        };
        let type_id = type_ins.instance_of();

        match type_id.0 {
            INT_ID => Type::Int,
            STRING_ID => Type::String,
            BOOL_ID => Type::Regular(vec![
                RawCase::new(Constructor::False, Vec::new()),
                RawCase::new(Constructor::True, Vec::new()),
            ]),
            ARRAY_ID => {
                let args = type_ins.type_arguments(self.db()).unwrap().clone();
                let raw_type = args.values().next().unwrap();
                let val_type =
                    TypeResolver::new(&mut self.state.db, &args, &self.bounds)
                        .resolve(raw_type)
                        .cast_according_to(self.db(), typ);

                Type::Array(val_type)
            }
            _ if type_id
                == self.db().type_in_module(BYTES_MODULE, BYTE_ARRAY_TYPE) =>
            {
                Type::Array(TypeRef::int())
            }
            // Slice[String] is treated as a String for the purpose of pattern
            // matching.
            _ if type_id
                == self.db().type_in_module(BYTES_MODULE, SLICE_TYPE) =>
            {
                Type::String
            }
            _ => match type_id.kind(self.db()) {
                TypeKind::Enum => {
                    let cons = type_id
                        .constructors(self.db())
                        .into_iter()
                        .map(|cons| {
                            let args = cons.arguments(self.db()).to_vec();
                            let cons = Constructor::Constructor(cons);
                            let vars = self.new_variables(type_ins, typ, args);

                            RawCase::new(cons, vars)
                        })
                        .collect();

                    Type::Enum(cons)
                }
                TypeKind::Regular | TypeKind::Extern => {
                    let fields = type_id.fields(self.db());
                    let args = fields
                        .iter()
                        .map(|f| f.value_type(self.db()))
                        .collect();

                    Type::Regular(vec![RawCase::new(
                        Constructor::Class(fields),
                        self.new_variables(type_ins, typ, args),
                    )])
                }
                TypeKind::Tuple => {
                    let fields = type_id.fields(self.db());
                    let args = fields
                        .iter()
                        .map(|f| f.value_type(self.db()))
                        .collect();

                    Type::Regular(vec![RawCase::new(
                        Constructor::Tuple(fields),
                        self.new_variables(type_ins, typ, args),
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
    use location::Location;
    use similar_asserts::assert_eq;
    use types::module_name::ModuleName;
    use types::{
        Module, Symbol, Type, TypeEnum, TypeId, TypeInstance, TypeKind,
        Variable as VariableType, Visibility,
    };

    fn expr(value: i64) -> hir::Expression {
        hir::Expression::Int(Box::new(hir::IntLiteral {
            resolved_type: types::TypeRef::Unknown,
            value,
            location: Location::default(),
        }))
    }

    fn state() -> State {
        let mut state = State::new(Config::new());
        let bmod = Module::alloc(
            &mut state.db,
            ModuleName::new(BYTES_MODULE),
            "bytes.inko".into(),
        );

        for name in [BYTE_ARRAY_TYPE, SLICE_TYPE] {
            let vis = Visibility::Public;
            let kind = TypeKind::Regular;
            let loc = Location::default();
            let name = name.to_string();
            let typ =
                Type::alloc(&mut state.db, name.clone(), kind, vis, bmod, loc);
            bmod.new_symbol(&mut state.db, name, Symbol::Type(typ));
        }

        state
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

    fn compiler(state: &mut State) -> Compiler<'_> {
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

    fn tt() -> Pattern {
        Pattern::Constructor(Constructor::True, Vec::new())
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
    fn test_unreachable_int() {
        let mut state = state();
        let mut compiler = compiler(&mut state);
        let input = compiler.new_variable(TypeRef::int());
        let result = compiler.compile(rules(
            input,
            vec![
                (Pattern::Int(4), BlockId(1)),
                (Pattern::Wildcard, BlockId(2)),
                (Pattern::Int(5), BlockId(3)),
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
                        success(BlockId(1))
                    ),
                    Case::new(
                        Constructor::Int(5),
                        Vec::new(),
                        success_with_bindings(
                            vec![Binding::Ignored(input)],
                            BlockId(2)
                        )
                    )
                ],
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
    fn test_unreachable_string() {
        let mut state = state();
        let mut compiler = compiler(&mut state);
        let input = compiler.new_variable(TypeRef::string());
        let result = compiler.compile(rules(
            input,
            vec![
                (Pattern::String("a".to_string()), BlockId(1)),
                (Pattern::Wildcard, BlockId(2)),
                (Pattern::String("b".to_string()), BlockId(3)),
            ],
        ));

        assert_eq!(
            result.tree,
            Decision::Switch(
                input,
                vec![
                    Case::new(
                        Constructor::String("a".to_string()),
                        Vec::new(),
                        success(BlockId(1))
                    ),
                    Case::new(
                        Constructor::String("b".to_string()),
                        Vec::new(),
                        success_with_bindings(
                            vec![Binding::Ignored(input)],
                            BlockId(2)
                        )
                    ),
                ],
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
        let loc = Location::default();
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
        let option_type = Type::alloc(
            &mut state.db,
            "Option".to_string(),
            TypeKind::Enum,
            Visibility::Public,
            module,
            Location::default(),
        );
        let some = option_type.new_constructor(
            &mut state.db,
            "Some".to_string(),
            vec![TypeRef::int()],
            Location::default(),
        );
        let _none = option_type.new_constructor(
            &mut state.db,
            "None".to_string(),
            Vec::new(),
            Location::default(),
        );
        let mut compiler = compiler(&mut state);
        let input = compiler.new_variable(TypeRef::Owned(
            TypeEnum::TypeInstance(TypeInstance::new(option_type)),
        ));
        let int_var = Variable(1);
        let result = compiler.compile(rules(
            input,
            vec![(
                Pattern::Constructor(
                    Constructor::Constructor(some),
                    vec![Pattern::Int(4)],
                ),
                BlockId(1),
            )],
        ));

        assert_eq!(
            result.tree,
            Decision::Switch(
                input,
                vec![Case::new(
                    Constructor::Constructor(some),
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
                )],
                Some(Box::new(fail()))
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
        let option_type = Type::alloc(
            &mut state.db,
            "Option".to_string(),
            TypeKind::Enum,
            Visibility::Public,
            module,
            Location::default(),
        );
        let some = option_type.new_constructor(
            &mut state.db,
            "Some".to_string(),
            vec![TypeRef::int()],
            Location::default(),
        );
        let none = option_type.new_constructor(
            &mut state.db,
            "None".to_string(),
            Vec::new(),
            Location::default(),
        );
        let mut compiler = compiler(&mut state);
        let input = compiler.new_variable(TypeRef::Owned(
            TypeEnum::TypeInstance(TypeInstance::new(option_type)),
        ));
        let int_var = Variable(1);
        let result = compiler.compile(rules(
            input,
            vec![
                (
                    Pattern::Constructor(
                        Constructor::Constructor(some),
                        vec![Pattern::Wildcard],
                    ),
                    BlockId(1),
                ),
                (
                    Pattern::Constructor(
                        Constructor::Constructor(none),
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
                        Constructor::Constructor(some),
                        vec![int_var],
                        success_with_bindings(
                            vec![Binding::Ignored(int_var)],
                            BlockId(1)
                        ),
                    ),
                    Case::new(
                        Constructor::Constructor(none),
                        Vec::new(),
                        success(BlockId(2))
                    )
                ],
                None
            )
        );
    }

    #[test]
    fn test_constructor_with_wildcard() {
        let mut state = state();
        let module = Module::alloc(
            &mut state.db,
            ModuleName::new("test"),
            "test.inko".into(),
        );
        let option_type = Type::alloc(
            &mut state.db,
            "Option".to_string(),
            TypeKind::Enum,
            Visibility::Public,
            module,
            Location::default(),
        );
        let some = option_type.new_constructor(
            &mut state.db,
            "Some".to_string(),
            vec![TypeRef::int()],
            Location::default(),
        );
        let _none = option_type.new_constructor(
            &mut state.db,
            "None".to_string(),
            Vec::new(),
            Location::default(),
        );
        let mut compiler = compiler(&mut state);
        let input = compiler.new_variable(TypeRef::Owned(
            TypeEnum::TypeInstance(TypeInstance::new(option_type)),
        ));
        let int_var = Variable(1);
        let result = compiler.compile(rules(
            input,
            vec![
                (
                    Pattern::Constructor(
                        Constructor::Constructor(some),
                        vec![Pattern::Wildcard],
                    ),
                    BlockId(1),
                ),
                (Pattern::Wildcard, BlockId(2)),
            ],
        ));

        assert_eq!(
            result.tree,
            Decision::Switch(
                input,
                vec![Case::new(
                    Constructor::Constructor(some),
                    vec![int_var],
                    success_with_bindings(
                        vec![Binding::Ignored(int_var)],
                        BlockId(1)
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
    fn test_nonexhaustive_type() {
        let mut state = state();
        let module = Module::alloc(
            &mut state.db,
            ModuleName::new("test"),
            "test.inko".into(),
        );
        let person_type = Type::alloc(
            &mut state.db,
            "Person".to_string(),
            TypeKind::Regular,
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
            TypeEnum::TypeInstance(TypeInstance::new(person_type)),
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
        let tuple2 = Type::alloc(
            &mut state.db,
            "Tuple2".to_string(),
            TypeKind::Tuple,
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
            TypeEnum::TypeInstance(TypeInstance::new(tuple2)),
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
    fn test_exhaustive_type() {
        let mut state = state();
        let module = Module::alloc(
            &mut state.db,
            ModuleName::new("test"),
            "test.inko".into(),
        );
        let person_type = Type::alloc(
            &mut state.db,
            "Person".to_string(),
            TypeKind::Regular,
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
            TypeEnum::TypeInstance(TypeInstance::new(person_type)),
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
        let tuple2 = Type::alloc(
            &mut state.db,
            "Tuple2".to_string(),
            TypeKind::Tuple,
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
            TypeEnum::TypeInstance(TypeInstance::new(tuple2)),
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
        let tuple2 = Type::alloc(
            &mut state.db,
            "Tuple2".to_string(),
            TypeKind::Tuple,
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
            TypeEnum::TypeInstance(TypeInstance::new(tuple2)),
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
        let tuple2 = Type::alloc(
            &mut state.db,
            "Tuple2".to_string(),
            TypeKind::Tuple,
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
            TypeEnum::TypeInstance(TypeInstance::new(tuple2)),
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
        let tuple2 = Type::alloc(
            &mut state.db,
            "Tuple2".to_string(),
            TypeKind::Tuple,
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
            TypeEnum::TypeInstance(TypeInstance::new(tuple2)),
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

    #[test]
    fn test_simple_array_pattern() {
        let mut state = state();
        let mut compiler = compiler(&mut state);

        // The input of type Array[Bool].
        let par = TypeId::array()
            .new_type_parameter(compiler.db_mut(), "T".to_string());
        let mut targs = TypeArguments::new();

        targs.assign(par, TypeRef::boolean());

        let ints = TypeRef::Owned(TypeEnum::TypeInstance(
            TypeInstance::generic(compiler.db_mut(), TypeId::array(), targs),
        ));
        let input = compiler.new_variable(ints);
        let result = compiler.compile(rules(
            input,
            vec![
                (Pattern::Array(vec![tt()]), BlockId(1)),
                (Pattern::Array(vec![tt(), tt()]), BlockId(2)),
                (Pattern::Wildcard, BlockId(10)),
            ],
        ));

        assert_eq!(
            result.tree,
            Decision::SwitchArray(
                input,
                vec![
                    Case::new(
                        Constructor::Array(1),
                        vec![Variable(1)],
                        Decision::Switch(
                            Variable(1),
                            vec![
                                Case::new(
                                    Constructor::False,
                                    Vec::new(),
                                    success_with_bindings(
                                        vec![Binding::Ignored(input)],
                                        BlockId(10)
                                    ),
                                ),
                                Case::new(
                                    Constructor::True,
                                    Vec::new(),
                                    success(BlockId(1)),
                                ),
                            ],
                            None,
                        )
                    ),
                    Case::new(
                        Constructor::Array(2),
                        vec![Variable(1), Variable(2)],
                        Decision::Switch(
                            Variable(2),
                            vec![
                                Case::new(
                                    Constructor::False,
                                    Vec::new(),
                                    success_with_bindings(
                                        vec![Binding::Ignored(input)],
                                        BlockId(10)
                                    ),
                                ),
                                Case::new(
                                    Constructor::True,
                                    Vec::new(),
                                    Decision::Switch(
                                        Variable(1),
                                        vec![
                                            Case::new(
                                                Constructor::False,
                                                Vec::new(),
                                                success_with_bindings(
                                                    vec![Binding::Ignored(
                                                        input
                                                    )],
                                                    BlockId(10)
                                                ),
                                            ),
                                            Case::new(
                                                Constructor::True,
                                                Vec::new(),
                                                success(BlockId(2)),
                                            ),
                                        ],
                                        None
                                    ),
                                ),
                            ],
                            None,
                        )
                    ),
                ],
                Box::new(success_with_bindings(
                    vec![Binding::Ignored(input)],
                    BlockId(10)
                ))
            )
        );
    }
}
