//! Compiler passes that operate on Inko's MIR.
use crate::diagnostics::DiagnosticId;
use crate::hir;
use crate::mir::pattern_matching as pmatch;
use crate::mir::{
    Block, BlockId, CastType, Class, Constant, Goto, Instruction, LocationId,
    Method, Mir, Module, RegisterId, SELF_ID,
};
use crate::state::State;
use ast::source_location::SourceLocation;
use std::collections::{HashMap, HashSet};
use std::iter::repeat_with;
use std::mem::swap;
use std::path::PathBuf;
use std::str::FromStr;
use types::format::format_type;
use types::module_name::ModuleName;
use types::{
    self, Block as _, ClassId, ConstantId, Location, MethodId, ModuleId,
    Symbol, TypeBounds, TypeRef, EQ_METHOD, FIELDS_LIMIT, OPTION_NONE,
    OPTION_SOME, RESULT_CLASS, RESULT_ERROR, RESULT_MODULE, RESULT_OK,
};

const SELF_NAME: &str = "self";

const MODULES_LIMIT: usize = u32::MAX as usize;
const CLASSES_LIMIT: usize = u32::MAX as usize;
const METHODS_LIMIT: usize = u32::MAX as usize;

fn modulo(lhs: i64, rhs: i64) -> Option<i64> {
    lhs.checked_rem(rhs)
        .and_then(|res| res.checked_add(rhs))
        .and_then(|res| res.checked_rem(rhs))
}

/// A compiler pass that verifies various global limits, such as the number of
/// defined classes.
pub(crate) fn check_global_limits(state: &mut State) -> Result<(), String> {
    let num_mods = state.db.number_of_modules();
    let num_classes = state.db.number_of_classes();
    let num_methods = state.db.number_of_methods();

    if num_mods > MODULES_LIMIT {
        return Err(format!(
            "the total number of modules ({}) \
            exceeds the maximum of {} modules",
            num_mods, MODULES_LIMIT
        ));
    }

    if num_classes > CLASSES_LIMIT {
        return Err(format!(
            "the total number of classes ({}) \
            exceeds the maximum of {} classes",
            num_classes, CLASSES_LIMIT
        ));
    }

    if num_methods > METHODS_LIMIT {
        return Err(format!(
            "the total number of methods ({}) \
            exceeds the maximum of {} methods",
            num_methods, METHODS_LIMIT
        ));
    }

    Ok(())
}

pub(crate) fn define_default_compile_time_variables(state: &mut State) {
    // "std.env" isn't imported by default, so only define the variables if this
    // is actually possible.
    if state.db.optional_module("std.env").is_none() {
        return;
    }

    let vars = [
        ("std.env", "ARCH", state.config.target.arch_name()),
        ("std.env", "OS", state.config.target.os_name()),
        ("std.env", "ABI", state.config.target.abi_name()),
    ];

    for (module, name, val) in vars {
        state.config.compile_time_variables.insert(
            (ModuleName::new(module), name.to_string()),
            val.to_string(),
        );
    }
}

pub(crate) fn apply_compile_time_variables(
    state: &State,
    mir: &mut Mir,
) -> Result<(), String> {
    for ((mod_name, const_name), val) in &state.config.compile_time_variables {
        let Some(Symbol::Constant(id)) = state
            .db
            .optional_module(mod_name.as_str())
            .and_then(|m| m.symbol(&state.db, const_name))
            .filter(|s| s.is_public(&state.db))
        else {
            return Err(format!(
                "the value of '{}.{}' can't be overwritten, either \
                because it doesn't exist or because it's a private constant",
                mod_name, const_name
            ));
        };

        let new = match mir.constants.get(&id).unwrap() {
            Constant::Int(_) => i64::from_str(val).ok().map(Constant::Int),
            Constant::String(_) => Some(Constant::String(val.clone())),
            Constant::Bool(_) => bool::from_str(val).ok().map(Constant::Bool),
            _ => {
                return Err(format!(
                    "the value of '{}.{}' can't be overwritten because its \
                    value is not an Int, String or Bool",
                    mod_name, const_name
                ));
            }
        };

        if let Some(val) = new {
            mir.constants.insert(id, val);
        } else {
            return Err(format!(
                "the value of '{}.{}' can't be overwritten because the new \
                value is invalid",
                mod_name, const_name
            ));
        }
    }

    Ok(())
}

enum Argument {
    Regular(hir::Argument),
    Input(hir::Expression, TypeRef),
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
enum RegisterState {
    /// The register is available, and should be dropped at the end of its
    /// surrounding scope.
    Available,

    /// The register has been moved, and shouldn't be dropped.
    Moved,

    /// The register contains a value of which one or more fields have been
    /// moved, but the containing value itself hasn't been moved.
    PartiallyMoved,

    /// The register is moved in one branch, while remaining live when taking
    /// another branch. Dropping of the register must be done conditionally.
    MaybeMoved,
}

/// The states of MIR registers, grouped per basic block.
///
/// The state is grouped per block as it may change between blocks. For example,
/// given the graph `A -> B`, a register may be available in `A` while it's
/// moved in `B`.
struct RegisterStates {
    mapping: HashMap<BlockId, HashMap<RegisterId, RegisterState>>,
}

impl RegisterStates {
    fn new() -> Self {
        Self { mapping: HashMap::new() }
    }

    fn set(
        &mut self,
        block: BlockId,
        register: RegisterId,
        state: RegisterState,
    ) {
        self.mapping.entry(block).or_default().insert(register, state);
    }

    fn get(
        &self,
        block: BlockId,
        register: RegisterId,
    ) -> Option<RegisterState> {
        self.mapping.get(&block).and_then(|m| m.get(&register)).cloned()
    }
}

#[derive(Copy, Clone, Debug)]
pub(crate) enum RegisterKind {
    /// A regular register to be dropped at the end of the surrounding scope.
    Regular,

    /// A temporary register introduced by pattern matching.
    ///
    /// These differ from regular registers in that if they are a value type,
    /// they should still be copied instead of used as-is.
    MatchVariable,

    /// A register introduced using a local variable.
    ///
    /// The stored `u32` value is the scope depth in which the variable is
    /// defined.
    Variable(types::VariableId, u32),

    /// A register introduced using a field.
    ///
    /// We store the field ID as part of this so we can mark it as moved. Field
    /// move states are stored separately, as field reads always produce new
    /// registers.
    Field(types::FieldId),

    /// A register introduced for `self`.
    ///
    /// These registers can't be moved if any fields have been moved.
    SelfObject,
}

impl RegisterKind {
    pub(crate) fn is_field(self) -> bool {
        matches!(self, RegisterKind::Field(_))
    }

    pub(crate) fn new_reference_on_return(self) -> bool {
        matches!(
            self,
            RegisterKind::Field(_)
                | RegisterKind::SelfObject
                | RegisterKind::Variable(_, _)
        )
    }

    pub(crate) fn is_regular(self) -> bool {
        matches!(self, RegisterKind::Regular)
    }

    fn name(self, db: &types::Database) -> Option<String> {
        match self {
            RegisterKind::Variable(id, _) => Some(id.name(db).clone()),
            RegisterKind::Field(id) => Some(id.name(db).clone()),
            RegisterKind::SelfObject => Some(SELF_NAME.to_string()),
            _ => None,
        }
    }
}

#[derive(Debug)]
enum ScopeKind {
    /// A regular scope.
    Regular,

    /// A scope introduced for a method call (chain).
    Call,

    /// The scope is created using the `loop` keyword.
    ///
    /// The values stored are the block `next` should jump to, and the block
    /// `break` should jump to.
    Loop(BlockId, BlockId),
}

struct Scope {
    kind: ScopeKind,
    parent: Option<Box<Scope>>,

    /// The registers created in this scope.
    created: Vec<RegisterId>,

    /// The scope depth, starting at 1.
    depth: u32,

    /// The depth of the surrounding loop.
    ///
    /// This value is set to zero if there's no loop surrounding the current
    /// scope.
    ///
    /// This value equals `depth` for the loop scope itself.
    loop_depth: u32,

    /// Registers that must be available at the end of a loop.
    ///
    /// This uses a HashMap as a register may be assigned a new value after it
    /// has been moved, only to be moved _again_. Using a Vec would result in
    /// outdated entries.
    moved_in_loop: HashMap<RegisterId, LocationId>,
}

impl Scope {
    fn root_scope() -> Box<Self> {
        Box::new(Self {
            kind: ScopeKind::Regular,
            created: Vec::new(),
            parent: None,
            depth: 1,
            loop_depth: 0,
            moved_in_loop: HashMap::new(),
        })
    }

    fn regular_scope(parent: &Scope) -> Box<Self> {
        Box::new(Self {
            kind: ScopeKind::Regular,
            created: Vec::new(),
            parent: None,
            depth: parent.depth + 1,
            loop_depth: parent.loop_depth,
            moved_in_loop: HashMap::new(),
        })
    }

    fn call_scope(parent: &Scope) -> Box<Self> {
        Box::new(Self {
            kind: ScopeKind::Call,
            created: Vec::new(),
            parent: None,
            depth: parent.depth + 1,
            loop_depth: parent.loop_depth,
            moved_in_loop: HashMap::new(),
        })
    }

    fn loop_scope(
        parent: &Scope,
        next_block: BlockId,
        break_block: BlockId,
    ) -> Box<Self> {
        let depth = parent.depth + 1;

        Box::new(Self {
            kind: ScopeKind::Loop(next_block, break_block),
            created: Vec::new(),
            parent: None,
            depth,
            loop_depth: depth,
            moved_in_loop: HashMap::new(),
        })
    }

    fn is_loop(&self) -> bool {
        matches!(self.kind, ScopeKind::Loop(_, _))
    }

    fn is_call(&self) -> bool {
        matches!(self.kind, ScopeKind::Call)
    }
}

/// A type describing the action to take when destructuring an object as part of
/// a pattern.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
enum RegisterAction {
    /// A field is to be moved into a new register.
    ///
    /// The wrapped value is the register that owned the field.
    Move(RegisterId),

    /// A field is to be incremented, and the reference moved into a new
    /// register.
    ///
    /// The wrapped value is the register that owned the field.
    Increment(RegisterId),
}

struct DecisionState {
    /// The register to write the results of a case body to.
    output: RegisterId,

    /// The block to jump to at the end of a case body.
    after_block: BlockId,

    /// The registers for all pattern matching variables, in the same order as
    /// the variables.
    registers: Vec<RegisterId>,

    /// The action to take per register when destructuring a value such as an
    /// enum variant of class.
    actions: HashMap<RegisterId, RegisterAction>,

    /// A mapping of parent registers to their child registers.
    ///
    /// The keys are the registers values are loaded from, and the values are
    /// the registers storing the child values. So when registers B and C
    /// contain sub values of A, the mapping is `A = [B, C]`.
    child_registers: HashMap<RegisterId, Vec<RegisterId>>,

    /// The basic blocks for every case body, and the code to compile for them.
    bodies: HashMap<
        BlockId,
        (Vec<hir::Expression>, Vec<RegisterId>, SourceLocation),
    >,

    /// The location of the `match` expression.
    location: LocationId,

    /// If the result of a match arm should be written to a register or ignored.
    write_result: bool,
}

impl DecisionState {
    fn new(
        output: RegisterId,
        after_block: BlockId,
        write_result: bool,
        location: LocationId,
    ) -> Self {
        Self {
            output,
            after_block,
            registers: Vec::new(),
            child_registers: HashMap::new(),
            actions: HashMap::new(),
            bodies: HashMap::new(),
            location,
            write_result,
        }
    }

    fn input_register(&self) -> RegisterId {
        self.registers[0]
    }

    fn load_child(
        &mut self,
        child: RegisterId,
        parent: RegisterId,
        action: RegisterAction,
    ) {
        self.actions.insert(child, action);
        self.child_registers.entry(parent).or_insert_with(Vec::new).push(child);
    }
}

pub(crate) struct GenerateDropper<'a> {
    pub(crate) state: &'a mut State,
    pub(crate) mir: &'a mut Mir,
    pub(crate) module: ModuleId,
    pub(crate) class: ClassId,
    pub(crate) location: LocationId,
}

impl<'a> GenerateDropper<'a> {
    pub(crate) fn run(mut self) -> MethodId {
        match self.class.kind(&self.state.db) {
            types::ClassKind::Async => self.async_class(),
            types::ClassKind::Enum => self.enum_class(),
            _ => self.regular_class(),
        }
    }

    /// Generates the dropper method for a regular class.
    ///
    /// This version runs the destructor (if any), followed by running the
    /// dropper of every field. Finally, it frees the receiver.
    fn regular_class(&mut self) -> MethodId {
        self.generate_dropper(
            types::DROPPER_METHOD,
            types::MethodKind::Mutable,
            true,
            false,
        )
    }

    /// Generates the dropper methods for an async class.
    ///
    /// Async classes are dropped asynchronously. This is achieved as follows:
    /// the regular dropper simply schedules an async version of the drop glue.
    /// Because this only runs when removing the last reference to the process,
    /// the async dropper is the last message. When run, it cleans up the object
    /// like a regular class, and the process shuts down.
    fn async_class(&mut self) -> MethodId {
        let loc = self.location;
        let async_dropper = self.generate_dropper(
            types::ASYNC_DROPPER_METHOD,
            types::MethodKind::AsyncMutable,
            false,
            true,
        );
        let dropper_type =
            self.method_type(types::DROPPER_METHOD, types::MethodKind::Mutable);
        let mut dropper_method = Method::new(dropper_type, loc);
        let mut lower = LowerMethod::new(
            self.state,
            self.mir,
            self.module,
            &mut dropper_method,
        );

        lower.prepare(loc);

        let self_reg = lower.self_register;
        let nil_reg = lower.get_nil(loc);

        // We don't need to increment here, because we only reach this point
        // when all references are gone and no messages are in flight any more,
        // thus no new messages can be produced.
        lower.current_block_mut().send(
            self_reg,
            async_dropper,
            Vec::new(),
            None,
            loc,
        );

        lower.current_block_mut().return_value(nil_reg, loc);
        self.add_method(types::DROPPER_METHOD, dropper_type, dropper_method);
        dropper_type
    }

    /// Generates the dropper method for an enum class.
    ///
    /// For enums the drop logic is a bit different: based on the value of the
    /// tag, certain fields may be set to NULL. As such we branch based on the
    /// tag value, and only drop the fields relevant for that tag.
    fn enum_class(&mut self) -> MethodId {
        let loc = self.location;
        let name = types::DROPPER_METHOD;
        let class = self.class;
        let drop_method_opt = class.method(&self.state.db, types::DROP_METHOD);
        let method_type = self.method_type(name, types::MethodKind::Mutable);
        let mut method = Method::new(method_type, loc);
        let mut lower =
            LowerMethod::new(self.state, self.mir, self.module, &mut method);

        lower.prepare(loc);

        let self_reg = lower.self_register;

        if let Some(id) = drop_method_opt {
            let typ = TypeRef::nil();
            let res = lower.new_register(typ);

            lower.current_block_mut().call_instance(
                res,
                self_reg,
                id,
                Vec::new(),
                None,
                loc,
            );
        }

        let variants = class.variants(lower.db());
        let mut blocks = Vec::new();
        let before_block = lower.current_block;
        let after_block = lower.add_block();
        let enum_fields = class.enum_fields(lower.db());
        let tag_field =
            class.field_by_index(lower.db(), types::ENUM_TAG_INDEX).unwrap();
        let tag_reg = lower.new_register(TypeRef::int());

        for var in variants {
            let block = lower.add_current_block();

            lower.add_edge(before_block, block);

            let members = var.members(lower.db());
            let fields = &enum_fields[0..members.len()];

            for (&field, typ) in fields.iter().zip(members.into_iter()).rev() {
                let reg = lower.new_register(typ);

                lower
                    .current_block_mut()
                    .get_field(reg, self_reg, class, field, loc);
                lower.drop_register(reg, loc);
            }

            lower.current_block_mut().goto(after_block, loc);
            lower.add_edge(lower.current_block, after_block);
            blocks.push(block);
        }

        lower
            .block_mut(before_block)
            .get_field(tag_reg, self_reg, class, tag_field, loc);
        lower.block_mut(before_block).switch(tag_reg, blocks, loc);

        lower.current_block = after_block;

        // Destructors may introduce new references, so we have to check again.
        // We do this _after_ processing fields so we can correctly drop cyclic
        // types.
        lower.current_block_mut().check_refs(self_reg, loc);

        lower.drop_register(tag_reg, loc);
        lower.current_block_mut().free(self_reg, class, loc);

        let nil_reg = lower.get_nil(loc);

        lower.current_block_mut().return_value(nil_reg, loc);
        self.add_method(name, method_type, method);
        method_type
    }

    fn generate_dropper(
        &mut self,
        name: &str,
        kind: types::MethodKind,
        free_self: bool,
        terminate: bool,
    ) -> MethodId {
        let class = self.class;
        let drop_method_opt = class.method(&self.state.db, types::DROP_METHOD);
        let method_type = self.method_type(name, kind);
        let loc = self.location;
        let mut method = Method::new(method_type, loc);
        let mut lower =
            LowerMethod::new(self.state, self.mir, self.module, &mut method);

        lower.prepare(loc);

        let self_reg = lower.self_register;

        if let Some(id) = drop_method_opt {
            let typ = TypeRef::nil();
            let res = lower.new_register(typ);

            lower.current_block_mut().call_instance(
                res,
                self_reg,
                id,
                Vec::new(),
                None,
                loc,
            );
        }

        for field in class.fields(lower.db()).into_iter().rev() {
            let typ = field.value_type(lower.db());

            if typ.is_permanent(lower.db()) {
                continue;
            }

            let reg = lower.new_register(typ);

            lower
                .current_block_mut()
                .get_field(reg, self_reg, class, field, loc);

            lower.drop_register(reg, loc);
        }

        // Destructors may introduce new references, so we have to check again.
        // We do this _after_ processing fields so we can correctly drop cyclic
        // types.
        lower.current_block_mut().check_refs(self_reg, loc);

        if free_self {
            lower.current_block_mut().free(self_reg, class, loc);
        }

        if terminate {
            // No need to decrement here, because we only reach this point when
            // all references and pending messages are gone.
            lower.current_block_mut().finish(true, loc);
        } else {
            let nil_reg = lower.get_nil(loc);

            lower.current_block_mut().return_value(nil_reg, loc);
        }

        self.add_method(name, method_type, method);
        method_type
    }

    fn method_type(&mut self, name: &str, kind: types::MethodKind) -> MethodId {
        let loc = self.mir.location(self.location);
        let id = types::Method::alloc(
            &mut self.state.db,
            self.module,
            Location::new(loc.lines.clone(), loc.columns.clone()),
            name.to_string(),
            types::Visibility::TypePrivate,
            kind,
        );

        let self_type =
            types::TypeId::ClassInstance(types::ClassInstance::rigid(
                &mut self.state.db,
                self.class,
                &types::TypeBounds::new(),
            ));
        let receiver = TypeRef::Mut(self_type);

        id.set_receiver(&mut self.state.db, receiver);
        id.set_return_type(&mut self.state.db, TypeRef::nil());
        id
    }

    fn add_method(&mut self, name: &str, id: MethodId, method: Method) {
        let cid = self.class;

        cid.add_method(&mut self.state.db, name.to_string(), id);
        self.mir.classes.get_mut(&cid).unwrap().methods.push(id);
        self.mir.methods.insert(id, method);
    }
}

pub(crate) struct DefineConstants<'a> {
    state: &'a mut State,
    mir: &'a mut Mir,
    module_id: types::ModuleId,
}

impl<'a> DefineConstants<'a> {
    pub(crate) fn run_all(
        state: &mut State,
        mir: &mut Mir,
        modules: &Vec<hir::Module>,
    ) -> bool {
        // Literal constants are defined first, as binary constants may depend
        // on their values.
        for module in modules {
            let module_id = module.module_id;

            DefineConstants { state, mir, module_id }.define_literal(module);
        }

        for module in modules {
            let module_id = module.module_id;

            DefineConstants { state, mir, module_id }.define_binary(module);
        }

        !state.diagnostics.has_errors()
    }

    /// Defines constants who's values are literals.
    fn define_literal(&mut self, module: &hir::Module) {
        for expr in &module.expressions {
            if let hir::TopLevelExpression::Constant(n) = expr {
                let id = n.constant_id.unwrap();
                let val = match n.value {
                    hir::ConstExpression::Int(ref n) => Constant::Int(n.value),
                    hir::ConstExpression::String(ref n) => {
                        Constant::String(n.value.clone())
                    }
                    hir::ConstExpression::Float(ref n) => {
                        Constant::Float(n.value)
                    }
                    _ => continue,
                };

                self.mir.constants.insert(id, val);
            }
        }
    }

    /// Defines constants who's values are binary expressions.
    fn define_binary(&mut self, module: &hir::Module) {
        for expr in &module.expressions {
            if let hir::TopLevelExpression::Constant(n) = expr {
                let id = n.constant_id.unwrap();
                let val = self.expression(&n.value);

                self.mir.constants.insert(id, val);
            }
        }
    }

    fn expression(&mut self, node: &hir::ConstExpression) -> Constant {
        match node {
            hir::ConstExpression::Int(ref n) => Constant::Int(n.value),
            hir::ConstExpression::String(ref n) => {
                Constant::String(n.value.clone())
            }
            hir::ConstExpression::Float(ref n) => Constant::Float(n.value),
            hir::ConstExpression::Binary(ref n) => self.binary(n),
            hir::ConstExpression::True(_) => Constant::Bool(true),
            hir::ConstExpression::False(_) => Constant::Bool(false),
            hir::ConstExpression::ConstantRef(ref n) => match n.kind {
                types::ConstantKind::Constant(id) => {
                    self.mir.constants.get(&id).cloned().unwrap()
                }
                _ => unreachable!(),
            },
            hir::ConstExpression::Array(ref n) => Constant::Array(
                n.values.iter().map(|n| self.expression(n)).collect(),
            ),
        }
    }

    fn binary(&mut self, node: &hir::ConstBinary) -> Constant {
        let left = self.expression(&node.left);
        let right = self.expression(&node.right);
        let op = node.operator;
        let loc = &node.location;

        match left {
            Constant::Int(lhs) => {
                let mut res = None;

                if let Constant::Int(rhs) = right {
                    res = match op {
                        hir::Operator::Add => lhs.checked_add(rhs),
                        hir::Operator::BitAnd => Some(lhs & rhs),
                        hir::Operator::BitOr => Some(lhs | rhs),
                        hir::Operator::BitXor => Some(lhs ^ rhs),
                        hir::Operator::Div => lhs.checked_div(rhs),
                        hir::Operator::Mod => modulo(lhs, rhs),
                        hir::Operator::Mul => lhs.checked_mul(rhs),
                        hir::Operator::Pow => Some(lhs.pow(rhs as u32)),
                        hir::Operator::Shl => lhs.checked_shl(rhs as u32),
                        hir::Operator::Shr => lhs.checked_shr(rhs as u32),
                        hir::Operator::UnsignedShr => (lhs as u64)
                            .checked_shr(rhs as u32)
                            .map(|v| v as i64),
                        hir::Operator::Sub => lhs.checked_sub(rhs),
                        _ => None,
                    };
                }

                if let Some(val) = res {
                    Constant::Int(val)
                } else {
                    self.const_expr_error(&left, op, &right, loc);
                    Constant::Int(0)
                }
            }
            Constant::Float(lhs) => {
                let mut res = None;

                if let Constant::Float(rhs) = right {
                    res = match op {
                        hir::Operator::Add => Some(lhs + rhs),
                        hir::Operator::Div => Some(lhs / rhs),
                        hir::Operator::Mod => Some(((lhs % rhs) + rhs) % rhs),
                        hir::Operator::Mul => Some(lhs * rhs),
                        hir::Operator::Pow => Some(lhs.powf(rhs)),
                        hir::Operator::Sub => Some(lhs - rhs),
                        _ => None,
                    };
                }

                if let Some(val) = res {
                    Constant::Float(val)
                } else {
                    self.const_expr_error(&left, op, &right, loc);
                    Constant::Float(0.0)
                }
            }
            Constant::String(ref lhs) => {
                let mut res = None;

                if let Constant::String(ref rhs) = right {
                    if node.operator == hir::Operator::Add {
                        res = Some(format!("{}{}", lhs, rhs))
                    }
                }

                if let Some(val) = res {
                    Constant::String(val)
                } else {
                    self.const_expr_error(&left, op, &right, loc);
                    Constant::String(String::new())
                }
            }
            Constant::Array(_) | Constant::Bool(_) => {
                self.state.diagnostics.error(
                    DiagnosticId::InvalidConstExpr,
                    "constant Array and Bool values don't support \
                    binary operations",
                    self.file(),
                    node.location.clone(),
                );

                left
            }
        }
    }

    fn db(&self) -> &types::Database {
        &self.state.db
    }

    fn file(&self) -> PathBuf {
        self.module_id.file(self.db())
    }

    fn const_expr_error(
        &mut self,
        lhs: &Constant,
        operator: hir::Operator,
        rhs: &Constant,
        location: &SourceLocation,
    ) {
        self.state.diagnostics.invalid_const_expression(
            &lhs.to_string(),
            operator.method_name(),
            &rhs.to_string(),
            self.file(),
            location.clone(),
        );
    }
}

/// A compiler pass that lowers the HIR of all modules to MIR.
pub(crate) struct LowerToMir<'a> {
    state: &'a mut State,
    mir: &'a mut Mir,
    module: ModuleId,
}

impl<'a> LowerToMir<'a> {
    pub(crate) fn run_all(
        state: &mut State,
        mir: &mut Mir,
        nodes: Vec<hir::Module>,
    ) -> bool {
        let mut modules = Vec::new();
        let mut mod_types = Vec::new();
        let mut mod_nodes = Vec::new();

        // Traits and classes must be lowered first, so we can process
        // implementations later.
        for module in nodes {
            let (types, rest) = module.expressions.into_iter().partition(|v| {
                matches!(
                    v,
                    hir::TopLevelExpression::Trait(_)
                        | hir::TopLevelExpression::Class(_)
                        | hir::TopLevelExpression::ExternClass(_)
                )
            });

            let id = module.module_id;

            mod_types.push(types);
            mod_nodes.push(rest);
            modules.push(id);
            mir.modules.insert(id, Module::new(id));
        }

        for (&module, nodes) in modules.iter().zip(mod_types.into_iter()) {
            LowerToMir { state, mir, module }.lower_types(nodes);
        }

        for (&module, nodes) in modules.iter().zip(mod_nodes.into_iter()) {
            LowerToMir { state, mir, module }.lower_rest(nodes);
        }

        !state.diagnostics.has_errors()
    }

    fn lower_types(&mut self, nodes: Vec<hir::TopLevelExpression>) {
        for expr in nodes {
            match expr {
                hir::TopLevelExpression::Trait(n) => {
                    self.define_trait(*n);
                }
                hir::TopLevelExpression::Class(n) => {
                    self.define_class(*n);
                }
                hir::TopLevelExpression::ExternClass(n) => {
                    self.define_extern_class(*n);
                }
                _ => {}
            }
        }
    }

    fn lower_rest(&mut self, nodes: Vec<hir::TopLevelExpression>) {
        let id = self.module;
        let mut mod_methods = Vec::new();

        for expr in nodes {
            match expr {
                hir::TopLevelExpression::Constant(n) => {
                    let mod_id = self.module;

                    self.mir
                        .modules
                        .get_mut(&mod_id)
                        .unwrap()
                        .constants
                        .push(n.constant_id.unwrap())
                }
                hir::TopLevelExpression::ModuleMethod(n) => {
                    mod_methods.push(self.define_module_method(*n));
                }
                hir::TopLevelExpression::Implement(n) => {
                    self.implement_trait(*n);
                }
                hir::TopLevelExpression::Reopen(n) => {
                    self.reopen_class(*n);
                }
                _ => {}
            }
        }

        let mod_class_id = id.class(self.db());
        let mut mod_class = Class::new(mod_class_id);

        mod_class.add_methods(&mod_methods);
        self.mir.add_methods(mod_methods);
        self.add_class(mod_class_id, mod_class);
    }

    fn define_trait(&mut self, node: hir::DefineTrait) {
        let mut methods = Vec::new();

        for expr in node.body {
            if let hir::TraitExpression::InstanceMethod(n) = expr {
                methods.push(self.define_instance_method(*n));
            }
        }

        self.mir.add_methods(methods);
    }

    fn implement_trait(&mut self, node: hir::ImplementTrait) {
        let class_id = node.class_instance.unwrap().instance_of();
        let trait_id = node.trait_instance.unwrap().instance_of();
        let mut methods = Vec::new();
        let mut names = HashSet::new();

        for expr in node.body {
            let method = self.define_instance_method(expr);

            names.insert(method.id.name(self.db()).clone());
            methods.push(method);
        }

        for id in trait_id.default_methods(self.db()) {
            if !names.contains(id.name(self.db())) {
                let mut method = self.mir.methods.get(&id).unwrap().clone();

                // We need to make sure to use the ID of the class'
                // implementation of the method, rather than the ID of the
                // method as defined in its source trait.
                method.id =
                    class_id.method(self.db(), id.name(self.db())).unwrap();

                methods.push(method);
            }
        }

        self.mir.classes.get_mut(&class_id).unwrap().add_methods(&methods);
        self.mir.add_methods(methods);
    }

    fn define_class(&mut self, node: hir::DefineClass) {
        let id = node.class_id.unwrap();
        let mut methods = Vec::new();

        for expr in node.body {
            match expr {
                hir::ClassExpression::InstanceMethod(n) => {
                    methods.push(self.define_instance_method(*n));
                }
                hir::ClassExpression::StaticMethod(n) => {
                    self.define_static_method(*n);
                }
                hir::ClassExpression::AsyncMethod(n) => {
                    methods.push(self.define_async_method(*n));
                }
                hir::ClassExpression::Variant(n) => {
                    methods.push(self.define_variant_method(*n, id));
                }
                _ => {}
            }
        }

        let mut class = Class::new(id);
        let loc = self.mir.add_location(node.location);

        class.add_methods(&methods);
        self.mir.add_methods(methods);
        self.add_class(id, class);

        GenerateDropper {
            state: self.state,
            mir: self.mir,
            module: self.module,
            class: id,
            location: loc,
        }
        .run();
    }

    fn define_extern_class(&mut self, node: hir::DefineExternClass) {
        let id = node.class_id.unwrap();

        self.add_class(id, Class::new(id));
    }

    fn reopen_class(&mut self, node: hir::ReopenClass) {
        let id = node.class_id.unwrap();
        let mut methods = Vec::new();

        for expr in node.body {
            match expr {
                hir::ReopenClassExpression::InstanceMethod(n) => {
                    methods.push(self.define_instance_method(*n));
                }
                hir::ReopenClassExpression::StaticMethod(n) => {
                    self.define_static_method(*n);
                }
                hir::ReopenClassExpression::AsyncMethod(n) => {
                    methods.push(self.define_async_method(*n));
                }
            }
        }

        self.mir.classes.get_mut(&id).unwrap().add_methods(&methods);
        self.mir.add_methods(methods);
    }

    fn define_module_method(
        &mut self,
        node: hir::DefineModuleMethod,
    ) -> Method {
        let id = node.method_id.unwrap();
        let loc = self.mir.add_location(node.location.clone());
        let mut method = Method::new(id, loc);

        LowerMethod::new(self.state, self.mir, self.module, &mut method)
            .run(node.body, loc);

        method
    }

    fn define_instance_method(
        &mut self,
        node: hir::DefineInstanceMethod,
    ) -> Method {
        let id = node.method_id.unwrap();
        let loc = self.mir.add_location(node.location.clone());
        let mut method = Method::new(id, loc);

        LowerMethod::new(self.state, self.mir, self.module, &mut method)
            .run(node.body, loc);

        method
    }

    fn define_async_method(&mut self, node: hir::DefineAsyncMethod) -> Method {
        let id = node.method_id.unwrap();
        let loc = self.mir.add_location(node.location.clone());
        let mut method = Method::new(id, loc);

        LowerMethod::new(self.state, self.mir, self.module, &mut method)
            .run(node.body, loc);

        method
    }

    fn define_static_method(&mut self, node: hir::DefineStaticMethod) {
        let id = node.method_id.unwrap();
        let loc = self.mir.add_location(node.location.clone());
        let mut method = Method::new(id, loc);

        LowerMethod::new(self.state, self.mir, self.module, &mut method)
            .run(node.body, loc);

        self.mir.methods.insert(id, method);
    }

    fn define_variant_method(
        &mut self,
        node: hir::DefineVariant,
        class: types::ClassId,
    ) -> Method {
        let id = node.method_id.unwrap();
        let variant_id = node.variant_id.unwrap();
        let loc = self.mir.add_location(node.location);
        let mut method = Method::new(id, loc);
        let fields = class.enum_fields(self.db());
        let bounds = TypeBounds::new();
        let ins = TypeRef::Owned(types::TypeId::ClassInstance(
            types::ClassInstance::rigid(self.db_mut(), class, &bounds),
        ));
        let mut lower =
            LowerMethod::new(self.state, self.mir, self.module, &mut method);

        lower.prepare(loc);

        let ins_reg = lower.new_register(ins);
        let tag_reg = lower.new_register(TypeRef::int());
        let tag_val = variant_id.id(lower.db()) as i64;
        let tag_field =
            class.field_by_index(lower.db(), types::ENUM_TAG_INDEX).unwrap();

        lower.current_block_mut().allocate(ins_reg, class, loc);
        lower.current_block_mut().int_literal(tag_reg, tag_val, loc);
        lower
            .current_block_mut()
            .set_field(ins_reg, class, tag_field, tag_reg, loc);

        for (arg, field) in
            id.arguments(lower.db()).into_iter().zip(fields.into_iter())
        {
            let reg = *lower.variable_mapping.get(&arg.variable).unwrap();

            lower
                .current_block_mut()
                .set_field(ins_reg, class, field, reg, loc);
            lower.mark_register_as_moved(reg);
        }

        lower.mark_register_as_moved(ins_reg);
        lower.mark_register_as_moved(tag_reg);
        lower.current_block_mut().return_value(ins_reg, loc);
        method
    }

    fn db(&self) -> &types::Database {
        &self.state.db
    }

    fn db_mut(&mut self) -> &mut types::Database {
        &mut self.state.db
    }

    fn add_class(&mut self, id: types::ClassId, class: Class) {
        let mod_id = self.module;

        self.mir.classes.insert(id, class);
        self.mir.modules.get_mut(&mod_id).unwrap().classes.push(id);
    }
}

/// A type that lowers the HIR of a single method to MIR.
pub(crate) struct LowerMethod<'a> {
    state: &'a mut State,
    mir: &'a mut Mir,
    module: ModuleId,
    method: &'a mut Method,
    scope: Box<Scope>,
    current_block: BlockId,

    /// The register containing the surrounding type/receiver of a method.
    surrounding_type_register: RegisterId,

    /// The register containing the value of `self`.
    ///
    /// In the case of a closure, this will be set to the outer `self` (i.e. the
    /// `self` that is captured, not the closure itself).
    self_register: RegisterId,

    /// The state of various registers, grouped per block that produced the
    /// state.
    register_states: RegisterStates,

    /// The types of registers.
    register_kinds: Vec<RegisterKind>,

    /// A mapping of variable type IDs to their MIR registers.
    variable_mapping: HashMap<types::VariableId, RegisterId>,

    /// The variables used in this method.
    used_variables: HashSet<types::VariableId>,

    /// The registers to write field values to.
    ///
    /// Field values may change between reads, so we can't just read a field
    /// once and then reuse the register. Instead, field access always writes to
    /// a register. We map fields to registers here so that for field A we
    /// always write to register R, removing the need for tracking field states
    /// separately.
    field_mapping: HashMap<types::FieldId, RegisterId>,

    /// The registers to write to when a register is moved.
    drop_flags: HashMap<RegisterId, RegisterId>,

    /// Variables to remap to field reads, and the types to expose the fields
    /// as.
    variable_fields: HashMap<types::VariableId, types::FieldId>,

    /// The number of fields that are moved.
    moved_fields: usize,
}

impl<'a> LowerMethod<'a> {
    fn new(
        state: &'a mut State,
        mir: &'a mut Mir,
        module: ModuleId,
        method: &'a mut Method,
    ) -> Self {
        let current_block = method.body.add_start_block();

        Self {
            state,
            mir,
            module,
            method,
            current_block,
            scope: Scope::root_scope(),
            variable_mapping: HashMap::new(),
            field_mapping: HashMap::new(),
            drop_flags: HashMap::new(),
            register_states: RegisterStates::new(),
            register_kinds: Vec::new(),
            surrounding_type_register: RegisterId(SELF_ID),
            self_register: RegisterId(SELF_ID),
            variable_fields: HashMap::new(),
            used_variables: HashSet::new(),
            moved_fields: 0,
        }
    }

    fn prepare(&mut self, location: LocationId) {
        self.define_base_registers(location);
    }

    fn run(mut self, nodes: Vec<hir::Expression>, location: LocationId) {
        self.prepare(location);
        self.lower_method_body(nodes, location);
    }

    fn run_with_captured_self(
        mut self,
        nodes: Vec<hir::Expression>,
        self_field: types::FieldId,
        self_type: TypeRef,
        location: LocationId,
    ) {
        self.prepare(location);
        self.define_captured_self_register(self_field, self_type, location);
        self.lower_method_body(nodes, location);
    }

    fn lower_method_body(
        mut self,
        nodes: Vec<hir::Expression>,
        location: LocationId,
    ) {
        if nodes.is_empty() {
            let reg = self.get_nil(location);

            self.end_of_method_body(reg, location);
            return;
        }

        let max = nodes.len() - 1;
        let ignore_ret = self.method.id.ignore_return_value(self.db());

        for (index, node) in nodes.into_iter().enumerate() {
            // Lowering unreachable code is pointless, so we abort if we
            // encounter unreachable code before reaching the last expression.
            if !self.in_connected_block() {
                self.warn_unreachable(node.location());
                return;
            }

            if index < max {
                self.expression(node);
                continue;
            }

            let loc = self.add_location(node.location().clone());
            let rets = node.returns_value();
            let ret = if rets && !ignore_ret {
                self.output_expression(node)
            } else {
                self.expression(node)
            };

            if !self.in_connected_block() {
                self.check_for_unused_variables();
                return;
            }

            let reg = if ignore_ret || !rets { self.get_nil(loc) } else { ret };

            self.end_of_method_body(reg, loc);
            return;
        }
    }

    fn end_of_method_body(
        mut self,
        register: RegisterId,
        location: LocationId,
    ) {
        self.mark_register_as_moved(register);
        self.partially_move_self_if_field(register);
        self.drop_all_registers();
        self.check_for_unused_variables();
        self.return_register(register, location);
    }

    fn define_base_registers(&mut self, location: LocationId) {
        // The first register in a method is reserved for the receiver of the
        // method (e.g. `self`). For closures this points to the generated
        // closure object, not the outer `self` as captured by the closure.
        //
        // Static/module methods don't have this argument passed in, so we don't
        // define the register. This is OK because the type-checker disallows
        // the use of `self` in these cases.
        let self_reg = if self.method.id.is_instance(self.db()) {
            let reg = self.new_self(self.method.id.receiver(self.db()));

            self.method.arguments.push(reg);
            Some(reg)
        } else {
            None
        };

        let mut args = Vec::new();

        for arg in self.method.id.arguments(self.db()) {
            let reg = self.new_variable(arg.variable);

            // Arguments are part of the public API due to the presence of named
            // arguments. This may result in arguments being unused by design
            // (i.e. when implementing a trait method of which not all arguments
            // apply to the implementation), without the ability to prefix them
            // with an underscore. As such, we consider arguments as used.
            self.used_variables.insert(arg.variable);
            self.method.arguments.push(reg);
            self.variable_mapping.insert(arg.variable, reg);
            args.push(reg);
        }

        if let Some(reg) = self_reg {
            self.add_drop_flag(reg, location);

            // Field registers have to be defined ahead of time so we can track
            // their state properly. This is only needed for instance methods.
            let rec = self.register_type(reg);

            for (id, typ) in self.method.id.fields(self.db()) {
                self.field_register(
                    id,
                    typ.cast_according_to(rec, self.db()),
                    location,
                );
            }
        }

        for reg in args {
            self.add_drop_flag(reg, location);
        }
    }

    fn define_captured_self_register(
        &mut self,
        field: types::FieldId,
        field_type: TypeRef,
        location: LocationId,
    ) {
        // Within a closure, explicit and implicit references to `self` should
        // use the _captured_ `self` (i.e. point to the outer `self` value), not
        // the closure itself.
        let captured = self.field_register(field, field_type, location);
        let closure = self.surrounding_type_register;
        let class = self.register_type(closure).class_id(self.db()).unwrap();

        self.current_block_mut()
            .get_field(captured, closure, class, field, location);
        self.self_register = captured;
    }

    fn body(
        &mut self,
        nodes: Vec<hir::Expression>,
        location: LocationId,
    ) -> RegisterId {
        let mut res = None;
        let max_index = if nodes.is_empty() { 0 } else { nodes.len() - 1 };

        for (index, n) in nodes.into_iter().enumerate() {
            if !self.in_connected_block() {
                self.warn_unreachable(n.location());
                break;
            }

            let reg = if index == max_index {
                self.output_expression(n)
            } else {
                self.expression(n)
            };

            res = Some(reg);
        }

        res.unwrap_or_else(|| self.get_nil(location))
    }

    fn expression(&mut self, node: hir::Expression) -> RegisterId {
        match node {
            hir::Expression::And(n) => self.binary_and(*n),
            hir::Expression::AssignField(n) => self.assign_field(*n),
            hir::Expression::ReplaceField(n) => self.replace_field(*n),
            hir::Expression::AssignSetter(n) => self.assign_setter(*n),
            hir::Expression::AssignVariable(n) => self.assign_variable(*n),
            hir::Expression::ReplaceVariable(n) => self.replace_variable(*n),
            hir::Expression::Break(n) => self.break_expression(*n),
            hir::Expression::BuiltinCall(n) => self.builtin_call(*n),
            hir::Expression::Call(n) => self.call(*n),
            hir::Expression::Closure(n) => self.closure(*n),
            hir::Expression::ConstantRef(n) => self.constant(*n),
            hir::Expression::DefineVariable(n) => self.define_variable(*n),
            hir::Expression::False(n) => self.false_literal(*n),
            hir::Expression::FieldRef(n) => self.field(*n),
            hir::Expression::Float(n) => self.float_literal(*n),
            hir::Expression::IdentifierRef(n) => self.identifier(*n),
            hir::Expression::Int(n) => self.int_literal(*n),
            hir::Expression::Loop(n) => self.loop_expression(*n),
            hir::Expression::Match(n) => self.match_expression(*n),
            hir::Expression::Mut(n) => self.mut_expression(*n),
            hir::Expression::Next(n) => self.next_expression(*n),
            hir::Expression::Or(n) => self.binary_or(*n),
            hir::Expression::Ref(n) => self.ref_expression(*n),
            hir::Expression::Return(n) => self.return_expression(*n),
            hir::Expression::Scope(n) => self.scope_expression(*n),
            hir::Expression::SelfObject(n) => self.self_expression(*n),
            hir::Expression::String(n) => self.string_literal(*n),
            hir::Expression::Throw(n) => self.throw_expression(*n),
            hir::Expression::True(n) => self.true_literal(*n),
            hir::Expression::Nil(n) => self.nil_literal(*n),
            hir::Expression::Tuple(n) => self.tuple_literal(*n),
            hir::Expression::TypeCast(n) => self.type_cast(*n),
            hir::Expression::Recover(n) => self.recover_expression(*n),
            hir::Expression::Try(n) => self.try_expression(*n),
            hir::Expression::Noop(n) => self.noop(n.location),
        }
    }

    fn binary_and(&mut self, node: hir::And) -> RegisterId {
        let loc = self.add_location(node.location);
        let reg = self.new_untracked_register(node.resolved_type);
        let before_id = self.current_block;
        let lhs_id = self.add_current_block();
        let rhs_id = self.add_block();
        let after_id = self.add_block();

        self.add_edge(before_id, lhs_id);
        self.enter_scope();

        let lhs_reg = self.expression(node.left);

        self.add_edge(self.current_block, rhs_id);
        self.add_edge(self.current_block, after_id);
        self.current_block_mut().move_register(reg, lhs_reg, loc);
        self.exit_scope();
        self.current_block_mut().branch(reg, rhs_id, after_id, loc);

        self.current_block = rhs_id;

        self.enter_scope();

        let rhs_reg = self.expression(node.right);

        self.current_block_mut().move_register(reg, rhs_reg, loc);
        self.add_edge(self.current_block, after_id);
        self.exit_scope();

        self.current_block = after_id;

        self.scope.created.push(reg);
        reg
    }

    fn binary_or(&mut self, node: hir::Or) -> RegisterId {
        let loc = self.add_location(node.location);
        let reg = self.new_untracked_register(node.resolved_type);
        let before_id = self.current_block;
        let lhs_id = self.add_current_block();
        let rhs_id = self.add_block();
        let after_id = self.add_block();

        self.add_edge(before_id, lhs_id);
        self.enter_scope();

        let lhs_reg = self.expression(node.left);

        self.add_edge(self.current_block, rhs_id);
        self.add_edge(self.current_block, after_id);
        self.current_block_mut().move_register(reg, lhs_reg, loc);
        self.exit_scope();
        self.current_block_mut().branch(reg, after_id, rhs_id, loc);

        self.current_block = rhs_id;

        self.enter_scope();

        let rhs_reg = self.expression(node.right);

        self.current_block_mut().move_register(reg, rhs_reg, loc);
        self.add_edge(self.current_block, after_id);
        self.exit_scope();

        self.current_block = after_id;

        self.scope.created.push(reg);
        reg
    }

    fn loop_expression(&mut self, node: hir::Loop) -> RegisterId {
        let loc = self.add_location(node.location);
        let before_loop = self.current_block;
        let loop_start = self.add_current_block();
        let after_loop = self.add_block();

        self.add_edge(before_loop, loop_start);
        self.enter_loop_scope(loop_start, after_loop);

        for node in node.body {
            if !self.in_connected_block() {
                self.warn_unreachable(node.location());
                break;
            }

            self.expression(node);
        }

        let connected = self.in_connected_block();

        if connected {
            self.check_loop_moves();
        }

        self.exit_scope();

        if connected {
            self.add_edge(self.current_block, loop_start);
            self.current_block_mut().preempt(loc);
            self.current_block_mut().goto(loop_start, loc);
        }

        self.current_block = after_loop;

        if self.in_connected_block() {
            // Even though `loop` is typed as returning `Never`, we have to
            // produce `nil` here because we will reach this point when breaking
            // out of the loop. At that point we may then end up returning this
            // value (e.g. a `loop` in an `if` would return this value as part
            // of the `if`).
            self.get_nil(loc)
        } else {
            self.new_register(TypeRef::Never)
        }
    }

    fn check_loop_moves(&mut self) {
        let mut moved = HashMap::new();

        // We remove the existing list of registers such that we don't produce
        // duplicate errors when moving in a loop that containes a `next` _and_
        // at some point breaks out of the loop.
        swap(&mut moved, &mut self.scope.moved_in_loop);

        for (reg, loc) in moved {
            if self.register_is_available(reg) {
                continue;
            }

            if let Some(name) = self.register_kind(reg).name(self.db()) {
                self.state.diagnostics.moved_variable_in_loop(
                    &name,
                    self.file(),
                    self.mir.location(loc).clone(),
                );
            }
        }
    }

    fn break_expression(&mut self, node: hir::Break) -> RegisterId {
        let target = self.loop_target().unwrap().1;

        self.jump_out_of_loop(target, node.location);
        self.new_register(TypeRef::Never)
    }

    fn next_expression(&mut self, node: hir::Next) -> RegisterId {
        let target = self.loop_target().unwrap().0;

        self.check_loop_moves();
        self.jump_out_of_loop(target, node.location);
        self.new_register(TypeRef::Never)
    }

    fn loop_target(&self) -> Option<(BlockId, BlockId)> {
        let mut scope = Some(&self.scope);

        while let Some(current) = scope {
            if let ScopeKind::Loop(next_block, break_block) = &current.kind {
                return Some((*next_block, *break_block));
            }

            scope = current.parent.as_ref();
        }

        None
    }

    fn jump_out_of_loop(&mut self, target: BlockId, location: SourceLocation) {
        let source = self.current_block;
        let loc = self.add_location(location);

        self.drop_loop_registers(loc);
        self.current_block_mut().preempt(loc);
        self.current_block_mut().goto(target, loc);
        self.add_edge(source, target);
        self.add_current_block();
    }

    fn tuple_literal(&mut self, node: hir::TupleLiteral) -> RegisterId {
        self.check_inferred(node.resolved_type, &node.location);

        let tup = self.new_register(node.resolved_type);
        let id = node.class_id.unwrap();
        let loc = self.add_location(node.location);
        let fields = id.fields(self.db());

        self.current_block_mut().allocate(tup, id, loc);

        for (index, val) in node.values.into_iter().enumerate() {
            let field = fields[index];
            let exp = node.value_types[index];
            let loc = self.add_location(val.location().clone());
            let reg = self.input_expression(val, Some(exp));

            self.current_block_mut().set_field(tup, id, field, reg, loc);
        }

        tup
    }

    fn true_literal(&mut self, node: hir::True) -> RegisterId {
        let loc = self.add_location(node.location);
        let reg = self.new_register(node.resolved_type);

        self.current_block_mut().true_literal(reg, loc);
        reg
    }

    fn false_literal(&mut self, node: hir::False) -> RegisterId {
        let loc = self.add_location(node.location);
        let reg = self.new_register(node.resolved_type);

        self.current_block_mut().false_literal(reg, loc);
        reg
    }

    fn nil_literal(&mut self, node: hir::Nil) -> RegisterId {
        let loc = self.add_location(node.location);

        self.get_nil(loc)
    }

    fn int_literal(&mut self, node: hir::IntLiteral) -> RegisterId {
        let loc = self.add_location(node.location);
        let reg = self.new_register(node.resolved_type);

        self.current_block_mut().int_literal(reg, node.value, loc);
        reg
    }

    fn float_literal(&mut self, node: hir::FloatLiteral) -> RegisterId {
        let loc = self.add_location(node.location);
        let reg = self.new_register(node.resolved_type);

        self.current_block_mut().float_literal(reg, node.value, loc);
        reg
    }

    fn string_literal(&mut self, mut node: hir::StringLiteral) -> RegisterId {
        match node.values.len() {
            0 => self.string_text(String::new(), node.location),
            1 => match node.values.pop().unwrap() {
                hir::StringValue::Text(n) => {
                    self.string_text(n.value, n.location)
                }
                hir::StringValue::Expression(n) => self.call(*n),
            },
            _ => {
                let mut vals = Vec::new();

                for val in node.values {
                    vals.push(match val {
                        hir::StringValue::Text(n) => {
                            self.string_text(n.value, n.location)
                        }
                        hir::StringValue::Expression(n) => self.call(*n),
                    });
                }

                let loc = self.add_location(node.location);
                let reg = self.new_register(node.resolved_type);

                self.current_block_mut().call_builtin(
                    reg,
                    types::BuiltinFunction::StringConcat,
                    vals,
                    loc,
                );
                reg
            }
        }
    }

    fn string_text(
        &mut self,
        value: String,
        location: SourceLocation,
    ) -> RegisterId {
        let reg = self.new_register(TypeRef::string());
        let loc = self.add_location(location);
        let block = self.current_block;

        self.permanent_string(reg, value, block, loc);
        reg
    }

    fn call(&mut self, node: hir::Call) -> RegisterId {
        let entered = self.enter_call_scope();
        let loc = self.add_location(node.name.location);
        let reg = match node.kind {
            types::CallKind::Call(info) => {
                self.check_inferred(info.returns, &node.location);

                let returns = info.returns;
                let rec = if info.receiver.is_explicit() {
                    node.receiver.map(|expr| self.expression(expr))
                } else {
                    None
                };

                let args =
                    node.arguments.into_iter().map(Argument::Regular).collect();

                let result = self.call_method(info, rec, args, loc);

                if returns.is_never(self.db()) {
                    self.add_current_block();
                }

                result
            }
            types::CallKind::GetField(info) => {
                self.check_inferred(info.variable_type, &node.location);

                let typ = info.variable_type;
                let rec = self.expression(node.receiver.unwrap());
                let reg = self.new_register(typ);

                if info.as_pointer {
                    self.current_block_mut()
                        .field_pointer(reg, rec, info.class, info.id, loc);
                } else {
                    self.current_block_mut()
                        .get_field(reg, rec, info.class, info.id, loc);
                }

                // When returning a field using the syntax `x.y`, we _must_ copy
                // or create a reference, otherwise it's possible to drop `x`
                // while the result of `y` is still in use.
                if typ.is_permanent(self.db()) || info.as_pointer {
                    reg
                } else if typ.is_value_type(self.db()) {
                    let copy = self.clone_value_type(reg, typ, true, loc);

                    self.mark_register_as_moved(reg);
                    self.mark_register_as_available(copy);
                    copy
                } else {
                    let ref_reg = self.new_register(typ);

                    self.current_block_mut().reference(ref_reg, reg, loc);
                    self.mark_register_as_moved(reg);
                    ref_reg
                }
            }
            types::CallKind::CallClosure(info) => {
                self.check_inferred(info.returns, &node.location);

                let returns = info.returns;
                let rec = self.expression(node.receiver.unwrap());
                let mut args = Vec::new();

                for arg in node.arguments.into_iter() {
                    if let hir::Argument::Positional(n) = arg {
                        let exp = n.expected_type;

                        args.push(self.input_expression(n.value, Some(exp)));
                    }
                }

                let res = self.new_register(returns);

                self.current_block_mut().call_closure(res, rec, args, loc);

                if returns.is_never(self.db()) {
                    self.add_current_block();
                }

                res
            }
            types::CallKind::ReadPointer(typ) => {
                let rec = self.expression(node.receiver.unwrap());
                let res = self.new_register(typ);

                self.current_block_mut().read_pointer(res, rec, loc);
                res
            }
            types::CallKind::GetConstant(id) => {
                let reg = self.new_register(id.value_type(self.db()));
                let loc = self.add_location(node.location);

                self.get_constant(reg, id, loc);
                reg
            }
            types::CallKind::ClassInstance(info) => {
                self.check_inferred(info.resolved_type, &node.location);

                let ins = self.new_register(info.resolved_type);
                let class = info.class_id;
                let loc = self.add_location(node.location);

                if class.kind(self.db()).is_async() {
                    self.current_block_mut().spawn(ins, class, loc);
                } else {
                    self.current_block_mut().allocate(ins, class, loc);
                }

                for (arg, (id, exp)) in
                    node.arguments.into_iter().zip(info.fields.into_iter())
                {
                    let loc = self.add_location(arg.location().clone());
                    let val =
                        self.input_expression(arg.into_value(), Some(exp));

                    self.current_block_mut()
                        .set_field(ins, class, id, val, loc);
                }

                ins
            }
            _ => unreachable!(),
        };

        self.exit_call_scope(entered, reg);
        reg
    }

    fn call_method(
        &mut self,
        info: types::CallInfo,
        receiver: Option<RegisterId>,
        arguments: Vec<Argument>,
        location: LocationId,
    ) -> RegisterId {
        let mut rec = match info.receiver {
            types::Receiver::Explicit => receiver.unwrap(),
            types::Receiver::Implicit => {
                let reg = self.self_register;

                if !self.register_is_available(self.self_register) {
                    let name = info.id.name(self.db()).clone();

                    self.state.diagnostics.implicit_receiver_moved(
                        &name,
                        self.file(),
                        self.mir.location(location).clone(),
                    );
                }

                reg
            }
            types::Receiver::Extern => {
                let arg_regs = self.call_arguments(info.id, arguments);
                let result = self.new_register(info.returns);

                self.current_block_mut()
                    .call_extern(result, info.id, arg_regs, location);

                if info.id.return_type(self.db()).is_never(self.db()) {
                    self.add_current_block();
                } else if !info.id.has_return_type(self.db()) {
                    self.current_block_mut().nil_literal(result, location);
                }

                // We don't reduce for extern calls for two reasons:
                //
                // 1. They are typically exposed through regular Inko
                //    methods, which would result in two reductions per call
                //    instead of one.
                // 2. This allows us to check `errno` after a call, without
                //    having to worry about the process being rescheduled in
                //    between the call and the check.
                return result;
            }
            types::Receiver::Class => {
                let arg_regs = self.call_arguments(info.id, arguments);
                let targs = self.mir.add_type_arguments(info.type_arguments);
                let result = self.new_register(info.returns);

                self.current_block_mut()
                    .call_static(result, info.id, arg_regs, targs, location);

                return result;
            }
        };

        // We must handle moving methods _before_ processing arguments, that way
        // we can prevent using the moved receiver as one of the arguments,
        // which would be unsound.
        if info.id.is_moving(self.db()) {
            rec = self.receiver_for_moving_method(rec, location);
        }

        // Argument registers must be defined _before_ the receiver register,
        // ensuring we drop them _after_ dropping the receiver (i.e in
        // reverse-lexical order).
        let arg_regs = self.call_arguments(info.id, arguments);
        let result = self.new_register(info.returns);
        let targs = self.mir.add_type_arguments(info.type_arguments);

        if info.id.is_async(self.db()) {
            // When sending messages we must increment the reference count,
            // otherwise we may end up scheduling the async dropper prematurely
            // (e.g. if new references are created before it runs).
            self.current_block_mut().increment_atomic(rec, location);
            self.current_block_mut()
                .send(rec, info.id, arg_regs, targs, location);

            self.current_block_mut().nil_literal(result, location);
        } else if info.dynamic {
            self.current_block_mut()
                .call_dynamic(result, rec, info.id, arg_regs, targs, location);
        } else {
            self.current_block_mut()
                .call_instance(result, rec, info.id, arg_regs, targs, location);
        }

        result
    }

    fn call_arguments(
        &mut self,
        method: MethodId,
        nodes: Vec<Argument>,
    ) -> Vec<RegisterId> {
        let mut args = vec![RegisterId(0); nodes.len()];

        for (index, arg) in nodes.into_iter().enumerate() {
            match arg {
                Argument::Regular(hir::Argument::Positional(n)) => {
                    args[index] = self.argument_expression(
                        method,
                        n.value,
                        Some(n.expected_type),
                    );
                }
                Argument::Regular(hir::Argument::Named(n)) => {
                    args[n.index] = self.argument_expression(
                        method,
                        n.value,
                        Some(n.expected_type),
                    );
                }
                Argument::Input(n, exp) => {
                    args[index] = self.input_expression(n, Some(exp));
                }
            }
        }

        args
    }

    fn check_inferred(&mut self, typ: TypeRef, location: &SourceLocation) {
        if typ.is_inferred(self.db()) {
            return;
        }

        self.state.diagnostics.cant_infer_type(
            format_type(self.db(), typ),
            self.file(),
            location.clone(),
        );
    }

    fn input_expression(
        &mut self,
        expression: hir::Expression,
        expected: Option<TypeRef>,
    ) -> RegisterId {
        let loc = self.add_location(expression.location().clone());
        let reg = self.expression(expression);
        let typ = self.register_type(reg);

        self.input_register(reg, typ, expected, loc)
    }

    fn argument_expression(
        &mut self,
        method: MethodId,
        expression: hir::Expression,
        expected: Option<TypeRef>,
    ) -> RegisterId {
        let loc = self.add_location(expression.location().clone());
        let reg = self.expression(expression);

        // Arguments passed to extern functions are passed as-is. This way we
        // can pass values to the runtime's functions, without adjusting
        // reference counts.
        if method.is_extern(self.db()) {
            return reg;
        }

        self.input_register(reg, self.register_type(reg), expected, loc)
    }

    fn assign_setter(&mut self, node: hir::AssignSetter) -> RegisterId {
        let entered = self.enter_call_scope();
        let reg = match node.kind {
            types::CallKind::Call(info) => {
                self.check_inferred(info.returns, &node.location);

                let loc = self.add_location(node.location);
                let rec = if info.receiver.is_explicit() {
                    Some(self.expression(node.receiver))
                } else {
                    None
                };

                let returns = info.returns;
                let args =
                    vec![Argument::Input(node.value, node.expected_type)];
                let result = self.call_method(info, rec, args, loc);

                if returns.is_never(self.db()) {
                    self.add_current_block();
                }

                result
            }
            types::CallKind::SetField(info) => {
                let rec = self.expression(node.receiver);
                let exp = info.variable_type;
                let loc = self.add_location(node.location);
                let arg = self.input_expression(node.value, Some(exp));
                let old = self.new_register(info.variable_type);

                if !info.variable_type.is_permanent(self.db()) {
                    self.current_block_mut()
                        .get_field(old, rec, info.class, info.id, loc);
                    self.drop_register(old, loc);
                }

                self.current_block_mut()
                    .set_field(rec, info.class, info.id, arg, loc);
                self.get_nil(loc)
            }
            types::CallKind::WritePointer => {
                let rec = self.expression(node.receiver);
                let arg = self.input_expression(node.value, None);
                let loc = self.add_location(node.location);

                self.current_block_mut().write_pointer(rec, arg, loc);
                self.get_nil(loc)
            }
            _ => unreachable!(),
        };

        self.exit_call_scope(entered, reg);
        reg
    }

    fn assign_variable(&mut self, node: hir::AssignVariable) -> RegisterId {
        let id = node.variable_id.unwrap();
        let exp = id.value_type(self.db());
        let loc = self.add_location(node.location);
        let val = self.input_expression(node.value, Some(exp));

        if let Some(&reg) = self.variable_mapping.get(&id) {
            if self.should_drop_register(reg) {
                self.drop_register(reg, loc);
            }

            self.mark_register_as_available(reg);
            self.current_block_mut().move_register(reg, val, loc);
        } else {
            let &field = self.variable_fields.get(&id).unwrap();
            let rec = self.surrounding_type_register;
            let class = self.register_type(rec).class_id(self.db()).unwrap();

            if !exp.is_permanent(self.db()) {
                // The captured variable may be exposed as a reference in `reg`,
                // but if the value is owned we need to drop it, not decrement
                // it.
                let old = self.new_register(exp);

                self.current_block_mut().get_field(old, rec, class, field, loc);
                self.drop_register(old, loc);
            }

            self.current_block_mut().set_field(rec, class, field, val, loc);
        }

        self.get_nil(loc)
    }

    fn replace_variable(&mut self, node: hir::ReplaceVariable) -> RegisterId {
        let id = node.variable_id.unwrap();
        let loc = self.add_location(node.location);
        let exp = node.resolved_type;
        let new_val = self.input_expression(node.value, Some(exp));
        let old_val = self.new_register(exp);

        if let Some(&reg) = self.variable_mapping.get(&id) {
            self.check_if_moved(
                reg,
                &node.variable.name,
                &node.variable.location,
            );

            self.current_block_mut().move_register(old_val, reg, loc);
            self.current_block_mut().move_register(reg, new_val, loc);
        } else {
            let &field = self.variable_fields.get(&id).unwrap();
            let rec = self.surrounding_type_register;
            let class = self.register_type(rec).class_id(self.db()).unwrap();

            self.current_block_mut().get_field(old_val, rec, class, field, loc);
            self.current_block_mut().set_field(rec, class, field, new_val, loc);
        }

        old_val
    }

    fn assign_field(&mut self, node: hir::AssignField) -> RegisterId {
        let id = node.field_id.unwrap();
        let loc = self.add_location(node.location);
        let exp = node.resolved_type;
        let new_val = self.input_expression(node.value, Some(exp));

        if let Some(&reg) = self.field_mapping.get(&id) {
            let rec = self.surrounding_type_register;
            let class = self.register_type(rec).class_id(self.db()).unwrap();
            let is_moved = self.register_is_moved(reg);

            if !is_moved && !exp.is_permanent(self.db()) {
                // `reg` may be a reference for a non-moving method, so we need
                // a temporary register using the raw field type and drop that
                // instead.
                let old = self.new_register(exp);

                self.current_block_mut().get_field(old, rec, class, id, loc);
                self.drop_register(old, loc);
            }

            // We allow the use of `self` again once all moved fields are
            // assigned a new value.
            if is_moved {
                self.moved_fields -= 1;

                if self.moved_fields == 0 {
                    self.mark_register_as_available(self.self_register);
                }
            }

            self.update_register_state(reg, RegisterState::Available);
            self.current_block_mut().set_field(rec, class, id, new_val, loc);
        } else {
            let rec = self.self_register;
            let class = self.register_type(rec).class_id(self.db()).unwrap();

            if !exp.is_permanent(self.db()) {
                let old = self.new_register(exp);

                // Closures capture `self` as a whole, so we can't end up with a
                // case where we try to drop an already dropped value here.
                self.current_block_mut().get_field(old, rec, class, id, loc);
                self.drop_register(old, loc);
            }

            self.current_block_mut().set_field(rec, class, id, new_val, loc);
        };

        self.get_nil(loc)
    }

    fn replace_field(&mut self, node: hir::ReplaceField) -> RegisterId {
        let id = node.field_id.unwrap();
        let loc = self.add_location(node.location);
        let exp = node.resolved_type;
        let new_val = self.input_expression(node.value, Some(exp));
        let old_val = self.new_register(exp);

        let (rec, check_reg) = if let Some(&reg) = self.field_mapping.get(&id) {
            (self.surrounding_type_register, reg)
        } else {
            (self.self_register, self.self_register)
        };

        let class = self.register_type(rec).class_id(self.db()).unwrap();

        self.check_if_moved(check_reg, &node.field.name, &node.field.location);
        self.current_block_mut().get_field(old_val, rec, class, id, loc);
        self.current_block_mut().set_field(rec, class, id, new_val, loc);
        old_val
    }

    fn builtin_call(&mut self, node: hir::BuiltinCall) -> RegisterId {
        let loc = self.add_location(node.location);
        let info = node.info.unwrap();
        let returns = info.returns;

        // Builtin calls don't take ownership of arguments, nor do we need/want
        // to modify reference counts. As such we use a simplified approach to
        // passing arguments (compared to regular method calls).
        let args: Vec<_> =
            node.arguments.into_iter().map(|n| self.expression(n)).collect();

        match info.id {
            types::BuiltinFunction::Moved => {
                self.mark_register_as_moved(args[0]);
                self.get_nil(loc)
            }
            name => {
                let reg = self.new_register(returns);

                // Builtin calls don't reduce as they're exposed through regular
                // methods, which already trigger reductions.
                self.current_block_mut().call_builtin(reg, name, args, loc);

                if returns.is_never(self.db()) {
                    self.add_current_block();
                }

                reg
            }
        }
    }

    fn return_expression(&mut self, node: hir::Return) -> RegisterId {
        let loc = self.add_location(node.location);
        let reg = if let Some(value) = node.value {
            self.output_expression(value)
        } else {
            self.get_nil(loc)
        };

        self.mark_register_as_moved(reg);
        self.drop_all_registers();
        self.return_register(reg, loc);
        self.add_current_block();
        self.new_register(TypeRef::Never)
    }

    fn try_expression(&mut self, node: hir::Try) -> RegisterId {
        let loc = self.add_location(node.location);
        let reg = self.expression(node.expression);
        let class = self.register_type(reg).class_id(self.db()).unwrap();
        let tag_reg = self.new_untracked_register(TypeRef::int());
        let tag_field =
            class.field_by_index(self.db(), types::ENUM_TAG_INDEX).unwrap();
        let val_field = class.enum_fields(self.db())[0];
        let ok_block = self.add_block();
        let err_block = self.add_block();
        let after_block = self.add_block();
        let mut blocks = vec![BlockId(0), BlockId(0)];
        let ret_reg = self.new_untracked_register(node.return_type);
        let err_tag = self.new_untracked_register(TypeRef::int());

        self.add_edge(self.current_block, ok_block);
        self.add_edge(self.current_block, err_block);
        self.add_edge(ok_block, after_block);

        self.mark_register_as_moved(reg);
        self.current_block_mut().get_field(tag_reg, reg, class, tag_field, loc);

        let out_reg = match node.kind {
            types::ThrowKind::Option(typ) => {
                let some_id = class
                    .variant(self.db(), OPTION_SOME)
                    .unwrap()
                    .id(self.db());
                let none_id = class
                    .variant(self.db(), OPTION_NONE)
                    .unwrap()
                    .id(self.db());
                let ok_reg = self.new_untracked_register(typ);

                blocks[some_id as usize] = ok_block;
                blocks[none_id as usize] = err_block;

                self.current_block_mut().switch(tag_reg, blocks, loc);

                // The block to jump to for a Some.
                self.block_mut(ok_block)
                    .get_field(ok_reg, reg, class, val_field, loc);
                self.block_mut(ok_block).drop_without_dropper(reg, loc);
                self.block_mut(ok_block).goto(after_block, loc);

                // The block to jump to for a None
                self.current_block = err_block;

                self.current_block_mut().allocate(ret_reg, class, loc);
                self.current_block_mut().int_literal(
                    err_tag,
                    none_id as _,
                    loc,
                );
                self.current_block_mut()
                    .set_field(ret_reg, class, tag_field, err_tag, loc);
                self.current_block_mut().drop_without_dropper(reg, loc);

                self.drop_all_registers();
                self.current_block_mut().return_value(ret_reg, loc);
                ok_reg
            }
            types::ThrowKind::Result(ok_typ, err_typ) => {
                let ok_id =
                    class.variant(self.db(), RESULT_OK).unwrap().id(self.db());
                let err_id = class
                    .variant(self.db(), RESULT_ERROR)
                    .unwrap()
                    .id(self.db());
                let ok_reg = self.new_untracked_register(ok_typ);
                let err_val = self.new_untracked_register(err_typ);

                blocks[ok_id as usize] = ok_block;
                blocks[err_id as usize] = err_block;

                self.current_block_mut().switch(tag_reg, blocks, loc);

                // The block to jump to for an Ok.
                self.block_mut(ok_block)
                    .get_field(ok_reg, reg, class, val_field, loc);
                self.block_mut(ok_block).drop_without_dropper(reg, loc);
                self.block_mut(ok_block).goto(after_block, loc);

                // The block to jump to for an Error.
                self.current_block = err_block;

                self.current_block_mut().allocate(ret_reg, class, loc);
                self.current_block_mut().int_literal(err_tag, err_id as _, loc);
                self.current_block_mut()
                    .get_field(err_val, reg, class, val_field, loc);
                self.current_block_mut()
                    .set_field(ret_reg, class, tag_field, err_tag, loc);
                self.current_block_mut()
                    .set_field(ret_reg, class, val_field, err_val, loc);
                self.current_block_mut().drop_without_dropper(reg, loc);

                self.drop_all_registers();
                self.current_block_mut().return_value(ret_reg, loc);
                ok_reg
            }
            _ => unreachable!(),
        };

        self.current_block = after_block;
        self.scope.created.push(out_reg);
        out_reg
    }

    fn noop(&mut self, location: SourceLocation) -> RegisterId {
        let loc = self.add_location(location);

        self.get_nil(loc)
    }

    fn throw_expression(&mut self, node: hir::Throw) -> RegisterId {
        let loc = self.add_location(node.location);
        let reg = self.expression(node.value);
        let class = self.db().class_in_module(RESULT_MODULE, RESULT_CLASS);
        let err_id =
            class.variant(self.db(), RESULT_ERROR).unwrap().id(self.db());
        let tag_field =
            class.field_by_index(self.db(), types::ENUM_TAG_INDEX).unwrap();
        let val_field = class.enum_fields(self.db())[0];
        let result_reg = self.new_register(node.return_type);
        let tag_reg = self.new_register(TypeRef::int());

        self.current_block_mut().allocate(result_reg, class, loc);
        self.current_block_mut().int_literal(tag_reg, err_id as _, loc);
        self.current_block_mut()
            .set_field(result_reg, class, tag_field, tag_reg, loc);
        self.current_block_mut()
            .set_field(result_reg, class, val_field, reg, loc);

        self.mark_register_as_moved(reg);
        self.mark_register_as_moved(result_reg);
        self.drop_all_registers();

        self.current_block_mut().return_value(result_reg, loc);

        self.add_current_block();
        self.new_register(TypeRef::Never)
    }

    fn return_register(&mut self, register: RegisterId, location: LocationId) {
        if self.method.id.is_async(self.db()) {
            let terminate = self.method.id.is_main(self.db());

            // The reference count is incremented before sending a message, so
            // we must also decrement it when we finish, and (if needed)
            // schedule the async dropper.
            self.drop_register(self.self_register, location);
            self.current_block_mut().finish(terminate, location);
        } else {
            self.current_block_mut().return_value(register, location);
        }
    }

    fn type_cast(&mut self, node: hir::TypeCast) -> RegisterId {
        let src = self.expression(node.value);
        let reg = self.new_register(node.resolved_type);
        let loc = self.add_location(node.location);
        let from_type = self.register_type(src);
        let to_type = node.resolved_type;

        match (
            CastType::from(self.db(), from_type),
            CastType::from(self.db(), to_type),
        ) {
            (CastType::Object, CastType::Object) => {
                let out = self.input_register(src, from_type, None, loc);

                self.current_block_mut().move_register(reg, out, loc);
            }
            (from @ CastType::Object, to) => {
                self.mark_register_as_moved(src);
                self.current_block_mut().cast(reg, src, from, to, loc);
            }
            (from, to) => {
                self.current_block_mut().cast(reg, src, from, to, loc);
            }
        }

        reg
    }

    fn ref_expression(&mut self, node: hir::Ref) -> RegisterId {
        self.increment(node.value, node.resolved_type, node.location)
    }

    fn mut_expression(&mut self, node: hir::Mut) -> RegisterId {
        if let Some(id) = node.pointer_to_method {
            let loc = self.add_location(node.location);
            let reg = self.new_register(node.resolved_type);

            self.current_block_mut().method_pointer(reg, id, loc);
            reg
        } else if node.resolved_type.is_pointer(self.db()) {
            let loc = self.add_location(node.location);
            let val = self.expression(node.value);
            let reg = self.new_register(node.resolved_type);

            self.current_block_mut().pointer(reg, val, loc);
            reg
        } else {
            self.increment(node.value, node.resolved_type, node.location)
        }
    }

    fn increment(
        &mut self,
        value: hir::Expression,
        return_type: TypeRef,
        location: SourceLocation,
    ) -> RegisterId {
        let loc = self.add_location(location);
        let val = self.expression(value);
        let val_type = self.register_type(val);

        if val_type.is_value_type(self.db()) {
            let reg = self.clone_value_type(val, return_type, false, loc);

            self.mark_register_as_available(reg);
            reg
        } else {
            let reg = self.new_register(return_type);

            self.current_block_mut().reference(reg, val, loc);
            reg
        }
    }

    fn recover_expression(&mut self, node: hir::Recover) -> RegisterId {
        self.enter_scope();

        let loc = self.add_location(node.location);
        let val = self.body(node.body, loc);

        self.mark_register_as_moved(val);
        self.exit_scope();

        let reg = self.new_register(node.resolved_type);

        self.current_block_mut().move_register(reg, val, loc);
        reg
    }

    fn scope_expression(&mut self, node: hir::Scope) -> RegisterId {
        self.enter_scope();

        let loc = self.add_location(node.location);
        let val = self.body(node.body, loc);

        self.mark_register_as_moved(val);
        self.exit_scope();

        let reg = self.new_register(node.resolved_type);

        self.current_block_mut().move_register(reg, val, loc);
        reg
    }

    fn define_variable(&mut self, node: hir::DefineVariable) -> RegisterId {
        let loc = self.add_location(node.location);
        let exp = node.resolved_type;

        if let Some(id) = node.variable_id {
            let src = self.input_expression(node.value, Some(exp));
            let reg = self.new_variable(id);

            self.variable_mapping.insert(id, reg);
            self.add_drop_flag(reg, loc);
            self.current_block_mut().move_register(reg, src, loc);
        } else {
            let src = self.input_expression(node.value, Some(exp));
            let reg = self.new_register(node.resolved_type);

            // We don't drop immediately as this would break e.g. guards bounds
            // to `_` (e.g. `let _ = something_that_returns_a_guard`).
            self.current_block_mut().move_register(reg, src, loc);
        }

        self.get_nil(loc)
    }

    fn match_expression(&mut self, node: hir::Match) -> RegisterId {
        let input_reg = self.input_expression(node.expression, None);
        let input_type = self.register_type(input_reg);

        // The result is untracked as otherwise an explicit return may drop it
        // before we write to it.
        let output_reg = self.new_untracked_register(node.resolved_type);

        let mut rows = Vec::new();
        let mut vars = pmatch::Variables::new();
        let input_var = vars.new_variable(input_type);
        let after_block = self.add_block();
        let loc = self.add_location(node.location.clone());
        let mut state =
            DecisionState::new(output_reg, after_block, node.write_result, loc);

        for case in node.cases {
            let var_regs = self.match_binding_registers(case.variable_ids);
            let block = self.add_block();
            let pat =
                pmatch::Pattern::from_hir(self.db(), self.mir, case.pattern);
            let col = pmatch::Column::new(input_var, pat);
            let body = pmatch::Body::new(block);

            state.bodies.insert(block, (case.body, var_regs, case.location));
            rows.push(pmatch::Row::new(vec![col], case.guard, body));
        }

        let bounds = self.method.id.bounds(self.db()).clone();
        let compiler = pmatch::Compiler::new(self.state, vars, bounds);
        let result = compiler.compile(rows);

        if result.missing {
            let missing = result.missing_patterns(self.db());

            self.state.diagnostics.error(
                DiagnosticId::InvalidMatch,
                format!(
                    "not all possible cases are covered, the following \
                    patterns are missing: {}",
                    missing
                        .into_iter()
                        .map(|v| format!("'{}'", v))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                self.file(),
                node.location,
            );

            return output_reg;
        }

        for typ in result.variables.types {
            state.registers.push(self.new_untracked_match_variable(typ));
        }

        self.current_block_mut().move_register(
            state.input_register(),
            input_reg,
            loc,
        );

        self.decision(&mut state, result.tree, self.current_block, Vec::new());

        for (_, _, loc) in state.bodies.into_values() {
            self.state.diagnostics.unreachable(self.file(), loc);
        }

        self.current_block = after_block;

        if !state.write_result {
            self.current_block_mut().nil_literal(output_reg, loc);
        }

        self.scope.created.push(output_reg);
        output_reg
    }

    fn decision(
        &mut self,
        state: &mut DecisionState,
        node: pmatch::Decision,
        parent_block: BlockId,
        registers: Vec<RegisterId>,
    ) -> BlockId {
        match node {
            pmatch::Decision::Success(body) => {
                let body_block = body.block_id;
                let vars_block = self.add_block();

                self.add_edge(parent_block, vars_block);
                self.decision_bindings(state, vars_block, body.bindings);
                self.drop_match_registers(state, registers);
                self.decision_body(state, self.current_block, body_block);
                vars_block
            }
            pmatch::Decision::Guard(guard_node, ok, fail) => {
                let guard = self.add_block();

                self.add_edge(parent_block, guard);
                self.enter_scope();

                // Bindings are defined _after_ the guard, otherwise the failure
                // case may try to bind/move registers already bound/moved
                // before running the guard. To allow referring to bindings in
                // the guard, we temporarily change the registers bindings refer
                // to.
                //
                // We don't need to increment if bindings capture references, as
                // this is done when the bindings are passed around.
                let mut restore = Vec::new();

                for bind in &ok.bindings {
                    if let pmatch::Binding::Named(id, pvar) = bind {
                        let new_reg = state.registers[pvar.0];
                        let old_reg =
                            self.variable_mapping.insert(*id, new_reg).unwrap();

                        restore.push((*id, old_reg, new_reg));
                    }
                }

                self.current_block = guard;

                let reg = self.expression(guard_node);

                self.exit_scope();

                for (id, old_reg, new_reg) in restore {
                    let state = self.register_state(new_reg);

                    self.update_register_state(old_reg, state);
                    self.variable_mapping.insert(id, old_reg);
                }

                let guard_end = self.current_block;
                let vars_block = self.add_block();
                let fail_block =
                    self.decision(state, *fail, guard_end, registers.clone());

                self.add_edge(guard_end, vars_block);
                self.block_mut(guard_end).branch(
                    reg,
                    vars_block,
                    fail_block,
                    state.location,
                );

                self.decision_bindings(state, vars_block, ok.bindings);

                // For guards we insert drop logic for intermediate registers
                // between the guard and the body, only running the code when
                // the guard matches. If we inject this code before running the
                // guard, we may drop registers used by the fallback branch of
                // the guard.
                self.drop_match_registers(state, registers);
                self.decision_body(state, self.current_block, ok.block_id);
                guard
            }
            pmatch::Decision::Switch(var, cases, fallback) => {
                let test = state.registers[var.0];

                match &cases[0].constructor {
                    pmatch::Constructor::True | pmatch::Constructor::False => {
                        self.bool_patterns(
                            state,
                            test,
                            cases,
                            parent_block,
                            registers,
                        )
                    }
                    pmatch::Constructor::Int(_) => self.int_patterns(
                        state,
                        test,
                        cases,
                        *fallback.unwrap(),
                        parent_block,
                        registers,
                    ),
                    pmatch::Constructor::String(_) => self.string_patterns(
                        state,
                        test,
                        cases,
                        *fallback.unwrap(),
                        parent_block,
                        registers,
                    ),
                    pmatch::Constructor::Tuple(_)
                    | pmatch::Constructor::Class(_) => self.class_patterns(
                        state,
                        test,
                        cases,
                        parent_block,
                        registers,
                    ),
                    pmatch::Constructor::Variant(_) => self.variant_patterns(
                        state,
                        test,
                        cases,
                        parent_block,
                        registers,
                    ),
                }
            }
            pmatch::Decision::Fail => {
                // We'll only reach this when the match is non-exhaustive, in
                // which case we don't progress to the next compilation stage.
                unreachable!()
            }
        }
    }

    fn decision_bindings(
        &mut self,
        state: &mut DecisionState,
        block: BlockId,
        bindings: Vec<pmatch::Binding>,
    ) {
        // This is needed to ensure register states are obtained for the correct
        // block.
        self.current_block = block;

        // We must enter a new scope before defining bindings, otherwise
        // they may be dropped by another match arm. It's expected that the
        // method used for processing decision bodies exits the scope.
        self.enter_scope();

        let loc = state.location;

        for bind in bindings {
            match bind {
                pmatch::Binding::Named(id, pvar) => {
                    let source = state.registers[pvar.0];
                    let target = *self.variable_mapping.get(&id).unwrap();

                    self.mark_register_as_moved(source);
                    self.add_drop_flag(target, loc);

                    match state.actions.get(&source) {
                        Some(&RegisterAction::Move(parent)) => {
                            // We mark the parent as _partially_ moved so we can
                            // still deallocate it, but know not to run its
                            // destructor.
                            self.mark_register_as_partially_moved(parent);
                            self.current_block_mut()
                                .move_register(target, source, loc);
                        }
                        Some(&RegisterAction::Increment(_)) => {
                            let typ = self.register_type(source);

                            if typ.is_value_type(self.db()) {
                                let copy = self
                                    .clone_value_type(source, typ, false, loc);

                                self.mark_register_as_moved(copy);
                                self.current_block_mut()
                                    .move_register(target, copy, loc);
                            } else {
                                self.current_block_mut()
                                    .reference(target, source, loc);
                            }
                        }
                        None => {
                            self.current_block_mut()
                                .move_register(target, source, loc);
                        }
                    }
                }
                pmatch::Binding::Ignored(pvar) => {
                    let reg = state.registers[pvar.0];

                    match state.actions.get(&reg) {
                        Some(&RegisterAction::Move(parent)) => {
                            self.mark_register_as_partially_moved(parent);

                            if self.register_type(reg).is_permanent(self.db()) {
                                self.mark_register_as_moved(reg);
                            } else {
                                self.drop_with_children(state, reg, loc);
                            }
                        }
                        None => {
                            if self.register_type(reg).is_permanent(self.db()) {
                                self.mark_register_as_moved(reg);
                            } else {
                                self.drop_with_children(state, reg, loc);
                            }
                        }
                        _ => self.mark_register_as_moved(reg),
                    }
                }
            }
        }
    }

    fn decision_body(
        &mut self,
        state: &mut DecisionState,
        parent_block: BlockId,
        start_block: BlockId,
    ) -> BlockId {
        self.add_edge(parent_block, start_block);

        // When a catch-all pattern is used (e.g. `case bla ...` or `case _
        // ...`), multiple nodes may jump to the body of this case. This check
        // ensures we only compile the code for the block once.
        let (exprs, mut var_regs, body_loc) =
            if let Some(val) = state.bodies.remove(&start_block) {
                val
            } else {
                // Don't forget to exit the scope here, since we entered a new
                // one bofer calling this method.
                self.exit_scope();

                return start_block;
            };

        self.current_block = start_block;

        self.scope.created.append(&mut var_regs);

        let loc = self.add_location(body_loc);
        let reg = self.body(exprs, loc);

        if state.write_result {
            self.mark_register_as_moved(reg);
        } else if self.in_connected_block() {
            self.drop_register(reg, loc);
        }

        // We don't enter a scope in this method, because we must enter a new
        // scope _before_ defining the match bindings, otherwise e.g. a `return`
        // could attempt to drop bindings from another match case.
        self.exit_scope();

        if self.in_connected_block() {
            if state.write_result {
                self.current_block_mut().move_register(state.output, reg, loc);
            }

            self.current_block_mut().goto(state.after_block, loc);
            self.add_edge(self.current_block, state.after_block);
        }

        start_block
    }

    fn drop_match_registers(
        &mut self,
        state: &DecisionState,
        mut registers: Vec<RegisterId>,
    ) {
        let loc = state.location;

        while let Some(reg) = registers.pop() {
            // We may encounter values partially moved, such as for the pattern
            // `(a, b)` where the surrounding tuple is partially moved.
            if self.register_is_moved(reg) {
                continue;
            }

            match state.actions.get(&reg) {
                Some(
                    &RegisterAction::Move(parent)
                    | &RegisterAction::Increment(parent),
                ) if self.register_is_moved(parent) => {
                    continue;
                }
                Some(&RegisterAction::Increment(_)) => {
                    // Registers are only incremented when bound. If we reach
                    // this point it means the register is never bound, and thus
                    // no dropping is needed.
                    self.mark_register_as_moved(reg);
                    continue;
                }
                _ => {}
            }

            self.mark_register_as_moved(reg);

            if self.register_type(reg).is_permanent(self.db()) {
                continue;
            }

            self.current_block_mut().drop_without_dropper(reg, loc);
        }
    }

    fn drop_with_children(
        &mut self,
        state: &mut DecisionState,
        register: RegisterId,
        location: LocationId,
    ) {
        self.drop_register(register, location);

        // The order in which child registers are dropped isn't consistent (i.e.
        // it could be A -> B -> C or B -> C -> A, or something else). Even if
        // it was, it would be in reverse order. This would mean we'd drop the
        // sub values, then try to drop the outer-most value by calling its
        // dropper, which would then try to drop already dropped data.
        //
        // To prevent this from happening, when we drop the value we also have
        // to recursively flag all child registers as moved.
        let mut work = if let Some(v) = state.child_registers.get(&register) {
            vec![v]
        } else {
            return;
        };

        while let Some(regs) = work.pop() {
            for reg in regs {
                self.mark_register_as_moved(*reg);

                if let Some(regs) = state.child_registers.get(reg) {
                    work.push(regs);
                }
            }
        }
    }

    fn bool_patterns(
        &mut self,
        state: &mut DecisionState,
        test_reg: RegisterId,
        cases: Vec<pmatch::Case>,
        parent_block: BlockId,
        registers: Vec<RegisterId>,
    ) -> BlockId {
        let loc = state.location;
        let block = self.add_block();

        self.add_edge(parent_block, block);

        let blocks: Vec<BlockId> = cases
            .into_iter()
            .map(|case| {
                self.decision(state, case.node, block, registers.clone())
            })
            .collect();

        self.block_mut(block).branch(test_reg, blocks[1], blocks[0], loc);
        block
    }

    fn string_patterns(
        &mut self,
        state: &mut DecisionState,
        test_reg: RegisterId,
        cases: Vec<pmatch::Case>,
        fallback_node: pmatch::Decision,
        parent_block: BlockId,
        mut registers: Vec<RegisterId>,
    ) -> BlockId {
        let blocks = self.add_blocks(cases.len());
        let loc = state.location;

        self.add_edge(parent_block, blocks[0]);
        self.connect_block_sequence(&blocks);
        registers.push(test_reg);

        let fallback = self.decision(
            state,
            fallback_node,
            *blocks.last().unwrap(),
            registers.clone(),
        );

        for (index, case) in cases.into_iter().enumerate() {
            let val = match case.constructor {
                pmatch::Constructor::String(val) => val,
                _ => unreachable!(),
            };

            let test_block = blocks[index];
            let fail_block = if let Some(&fail) = blocks.get(index + 1) {
                self.add_edge(test_block, fail);
                fail
            } else {
                fallback
            };

            let res_reg = self.new_untracked_register(TypeRef::boolean());
            let val_reg = self.new_untracked_register(TypeRef::string());
            let eq_method = ClassId::string()
                .method(self.db(), EQ_METHOD)
                .expect("String.== is undefined");

            self.permanent_string(val_reg, val, test_block, loc);
            self.block_mut(test_block).call_instance(
                res_reg,
                test_reg,
                eq_method,
                vec![val_reg],
                None,
                loc,
            );

            let ok_block =
                self.decision(state, case.node, test_block, registers.clone());

            self.block_mut(test_block)
                .branch(res_reg, ok_block, fail_block, loc);
        }

        blocks[0]
    }

    fn int_patterns(
        &mut self,
        state: &mut DecisionState,
        test_reg: RegisterId,
        cases: Vec<pmatch::Case>,
        fallback_node: pmatch::Decision,
        parent_block: BlockId,
        mut registers: Vec<RegisterId>,
    ) -> BlockId {
        let loc = state.location;
        let blocks = self.add_blocks(cases.len());

        self.add_edge(parent_block, blocks[0]);
        self.connect_block_sequence(&blocks);
        registers.push(test_reg);

        let fallback = self.decision(
            state,
            fallback_node,
            blocks[blocks.len() - 1],
            registers.clone(),
        );

        for (index, case) in cases.into_iter().enumerate() {
            let test_block = blocks[index];
            let fail_block = if let Some(&fail) = blocks.get(index + 1) {
                self.add_edge(test_block, fail);
                fail
            } else {
                fallback
            };

            let res_reg = self.new_untracked_register(TypeRef::boolean());

            let test_end_block = match case.constructor {
                pmatch::Constructor::Int(val) => {
                    let val_type = TypeRef::int();
                    let val_reg = self.new_untracked_register(val_type);

                    self.block_mut(test_block).int_literal(val_reg, val, loc);
                    self.block_mut(test_block).call_builtin(
                        res_reg,
                        types::BuiltinFunction::IntEq,
                        vec![test_reg, val_reg],
                        loc,
                    );

                    test_block
                }
                _ => unreachable!(),
            };

            let ok_block = self.decision(
                state,
                case.node,
                test_end_block,
                registers.clone(),
            );

            self.block_mut(test_end_block)
                .branch(res_reg, ok_block, fail_block, loc);
        }

        blocks[0]
    }

    fn class_patterns(
        &mut self,
        state: &mut DecisionState,
        test_reg: RegisterId,
        mut cases: Vec<pmatch::Case>,
        parent_block: BlockId,
        mut registers: Vec<RegisterId>,
    ) -> BlockId {
        let loc = state.location;
        let case = cases.pop().unwrap();
        let fields = match case.constructor {
            pmatch::Constructor::Tuple(v) => v,
            pmatch::Constructor::Class(v) => v,
            _ => unreachable!(),
        };

        let test_type = self.register_type(test_reg);

        registers.push(test_reg);

        for (arg, field) in case.arguments.into_iter().zip(fields.into_iter()) {
            let reg = state.registers[arg.0];
            let class =
                self.register_type(test_reg).class_id(self.db()).unwrap();

            let action = if test_type.is_owned_or_uni(self.db()) {
                RegisterAction::Move(test_reg)
            } else {
                RegisterAction::Increment(test_reg)
            };

            state.load_child(reg, test_reg, action);
            self.block_mut(parent_block)
                .get_field(reg, test_reg, class, field, loc);
        }

        self.decision(state, case.node, parent_block, registers)
    }

    fn variant_patterns(
        &mut self,
        state: &mut DecisionState,
        test_reg: RegisterId,
        cases: Vec<pmatch::Case>,
        parent_block: BlockId,
        mut registers: Vec<RegisterId>,
    ) -> BlockId {
        let loc = state.location;
        let test_block = self.add_block();
        let mut blocks = Vec::new();

        self.add_edge(parent_block, test_block);
        registers.push(test_reg);

        let test_type = self.register_type(test_reg);
        let class = test_type.class_id(self.db()).unwrap();
        let tag_reg = self.new_untracked_register(TypeRef::int());
        let tag_field =
            class.field_by_index(self.db(), types::ENUM_TAG_INDEX).unwrap();
        let member_fields = class.enum_fields(self.db());

        self.block_mut(test_block)
            .get_field(tag_reg, test_reg, class, tag_field, loc);

        for case in cases {
            let case_registers = registers.clone();
            let block = self.add_block();

            self.add_edge(test_block, block);
            blocks.push(block);

            for (arg, &field) in case.arguments.into_iter().zip(&member_fields)
            {
                let reg = state.registers[arg.0];
                let action = if test_type.is_owned_or_uni(self.db()) {
                    RegisterAction::Move(test_reg)
                } else {
                    RegisterAction::Increment(test_reg)
                };

                state.load_child(reg, test_reg, action);
                self.block_mut(block)
                    .get_field(reg, test_reg, class, field, loc);
            }

            self.decision(state, case.node, block, case_registers);
        }

        self.block_mut(test_block).switch(tag_reg, blocks, loc);
        test_block
    }

    fn identifier(&mut self, node: hir::IdentifierRef) -> RegisterId {
        let loc = self.add_location(node.location.clone());

        match node.kind {
            types::IdentifierKind::Variable(id) => {
                let reg = self.get_local(id, loc);

                self.check_if_moved(reg, &node.name, &node.location);
                reg
            }
            types::IdentifierKind::Method(info) => {
                let entered = self.enter_call_scope();
                let reg = self.call_method(info, None, Vec::new(), loc);

                self.exit_call_scope(entered, reg);
                reg
            }
            types::IdentifierKind::Field(info) => {
                if !self.register_is_available(self.self_register) {
                    self.state.diagnostics.implicit_receiver_moved(
                        &node.name,
                        self.file(),
                        self.mir.location(loc).clone(),
                    );
                }

                let rec = self.self_register;
                let reg = self.new_field(info.id, info.variable_type);

                self.current_block_mut()
                    .get_field(reg, rec, info.class, info.id, loc);
                reg
            }
            types::IdentifierKind::Unknown => unreachable!(),
        }
    }

    fn field(&mut self, node: hir::FieldRef) -> RegisterId {
        let loc = self.add_location(node.location.clone());
        let id = node.field_id.unwrap();
        let reg = if self.in_closure() {
            self.new_field(id, node.resolved_type)
        } else {
            self.field_mapping.get(&id).cloned().unwrap()
        };

        let rec = self.self_register;
        let class = self.register_type(rec).class_id(self.db()).unwrap();
        let name = &node.name;
        let check_loc = &node.location;

        match self.register_state(rec) {
            RegisterState::Available | RegisterState::PartiallyMoved => {
                self.check_if_moved(reg, name, check_loc);
            }
            _ => {
                self.state.diagnostics.implicit_receiver_moved(
                    name,
                    self.file(),
                    node.location.clone(),
                );
            }
        }

        if id.value_type(self.db()).is_stack_class_instance(self.db())
            && self.register_type(reg).is_pointer(self.db())
        {
            self.current_block_mut().field_pointer(reg, rec, class, id, loc);
        } else {
            self.current_block_mut().get_field(reg, rec, class, id, loc);
        }

        reg
    }

    fn constant(&mut self, node: hir::ConstantRef) -> RegisterId {
        match node.kind {
            types::ConstantKind::Constant(id) => {
                let reg = self.new_register(node.resolved_type);
                let loc = self.add_location(node.location);

                self.get_constant(reg, id, loc);
                reg
            }
            types::ConstantKind::Method(info) => {
                let entered = self.enter_call_scope();
                let loc = self.add_location(node.location);
                let reg = self.call_method(info, None, Vec::new(), loc);

                self.exit_call_scope(entered, reg);
                reg
            }
            _ => unreachable!(),
        }
    }

    fn self_expression(&mut self, node: hir::SelfObject) -> RegisterId {
        let reg = self.self_register;

        self.check_if_moved(reg, SELF_NAME, &node.location);
        reg
    }

    fn closure(&mut self, node: hir::Closure) -> RegisterId {
        self.check_inferred(node.resolved_type, &node.location);

        let module = self.module;
        let closure_id = node.closure_id.unwrap();
        let moving = closure_id.is_moving(self.db());
        let loc = Location::new(
            node.location.lines.clone(),
            node.location.columns.clone(),
        );
        let class_id = types::Class::alloc(
            self.db_mut(),
            format!("Closure{}", closure_id.0),
            types::ClassKind::Closure,
            types::Visibility::Private,
            module,
            loc,
        );

        let method_id = types::Method::alloc(
            self.db_mut(),
            module,
            Location::new(
                node.location.lines.clone(),
                node.location.columns.clone(),
            ),
            types::CALL_METHOD.to_string(),
            types::Visibility::Public,
            types::MethodKind::Mutable,
        );

        let gen_class_ins =
            types::TypeId::ClassInstance(types::ClassInstance::new(class_id));

        let call_rec_type = TypeRef::Mut(gen_class_ins);
        let returns = closure_id.return_type(self.db());

        method_id.set_receiver(self.db_mut(), call_rec_type);
        method_id.set_return_type(self.db_mut(), returns);

        for arg in closure_id.arguments(self.db()) {
            // As part of type checking a closure body, arguments and their
            // references use a certain set of VariableId values. We must reuse
            // these IDs for the generated method's arguments, otherwise the
            // `variable -> register` mapping is incomplete.
            method_id.add_argument(self.db_mut(), arg);
        }

        class_id.add_method(
            self.db_mut(),
            types::CALL_METHOD.to_string(),
            method_id,
        );

        let gen_class_type = TypeRef::Owned(gen_class_ins);
        let gen_class_reg = self.new_register(gen_class_type);
        let loc = self.add_location(node.location.clone());

        // We generate the allocation first, that way when we generate any
        // fields we can populate then right away, without having to store field
        // IDs.
        self.current_block_mut().allocate(gen_class_reg, class_id, loc);

        let mut field_index = 0;
        let field_vis = types::Visibility::TypePrivate;
        let mut captured_self_field = None;
        let mut variable_fields = HashMap::new();

        if let Some(mut captured_as) = closure_id.captured_self_type(self.db())
        {
            if !moving && captured_as.is_owned_or_uni(self.db()) {
                captured_as = captured_as.as_mut(self.db());
            }

            let exposed_as = if captured_as.is_owned_or_uni(self.db()) {
                captured_as.as_mut(self.db())
            } else {
                captured_as
            };

            let name = SELF_NAME.to_string();
            let field_loc = class_id.location(self.db());
            let field = class_id.new_field(
                self.db_mut(),
                name.clone(),
                field_index,
                captured_as,
                field_vis,
                module,
                field_loc,
            );

            let self_reg = self.self_register;

            if !self.register_is_available(self_reg) {
                self.state.diagnostics.moved_while_captured(
                    SELF_NAME,
                    self.file(),
                    node.location.clone(),
                );
            }

            let val = self.input_register(self_reg, captured_as, None, loc);

            self.current_block_mut().set_field(
                gen_class_reg,
                class_id,
                field,
                val,
                loc,
            );
            method_id.set_field_type(self.db_mut(), name, field, captured_as);

            captured_self_field = Some((field, exposed_as));
            field_index += 1;
        }

        for (var, captured_as) in closure_id.captured(self.db()) {
            let name = var.name(self.db()).clone();
            let field_loc = class_id.location(self.db());
            let field = class_id.new_field(
                self.db_mut(),
                name.clone(),
                field_index,
                captured_as,
                field_vis,
                module,
                field_loc,
            );

            let raw = self.get_local(var, loc);

            if !self.register_is_available(raw) {
                self.state.diagnostics.moved_while_captured(
                    &name,
                    self.file(),
                    node.location.clone(),
                );
            }

            let val = self.input_register(raw, captured_as, None, loc);

            self.current_block_mut().set_field(
                gen_class_reg,
                class_id,
                field,
                val,
                loc,
            );

            field_index += 1;

            method_id.set_field_type(self.db_mut(), name, field, captured_as);
            variable_fields.insert(var, field);
        }

        if field_index >= FIELDS_LIMIT {
            self.state.diagnostics.error(
                DiagnosticId::InvalidType,
                format!(
                    "closures can't capture more than {} variables",
                    FIELDS_LIMIT
                ),
                self.file(),
                node.location.clone(),
            );
        }

        let mut mir_class = Class::new(class_id);
        let mut mir_method = Method::new(method_id, loc);
        let mut lower = LowerMethod::new(
            self.state,
            self.mir,
            self.module,
            &mut mir_method,
        );

        lower.variable_fields = variable_fields;

        if let Some((id, typ)) = captured_self_field {
            lower.run_with_captured_self(node.body, id, typ, loc);
        } else {
            lower.run(node.body, loc);
        }

        let mod_id = self.module;

        mir_class.methods.push(method_id);
        self.mir.methods.insert(method_id, mir_method);
        self.mir.classes.insert(class_id, mir_class);
        self.mir.modules.get_mut(&mod_id).unwrap().classes.push(class_id);

        let loc = self.mir.add_location(node.location);

        GenerateDropper {
            state: self.state,
            mir: self.mir,
            module: self.module,
            class: class_id,
            location: loc,
        }
        .run();

        gen_class_reg
    }

    fn get_local(
        &mut self,
        id: types::VariableId,
        location: LocationId,
    ) -> RegisterId {
        self.mark_variable_as_used(id);

        if let Some(&reg) = self.variable_mapping.get(&id) {
            reg
        } else {
            let &field = self.variable_fields.get(&id).unwrap();
            let &reg = self.field_mapping.get(&field).unwrap();
            let rec = self.surrounding_type_register;
            let class = self.register_type(rec).class_id(self.db()).unwrap();

            self.current_block_mut()
                .get_field(reg, rec, class, field, location);
            reg
        }
    }

    fn get_nil(&mut self, location: LocationId) -> RegisterId {
        let reg = self.new_register(TypeRef::nil());

        self.current_block_mut().nil_literal(reg, location);
        reg
    }

    fn add_edge(&mut self, source: BlockId, target: BlockId) {
        self.method.body.add_edge(source, target);
    }

    fn connect_block_sequence(&mut self, blocks: &[BlockId]) {
        for (&curr, &next) in blocks.iter().zip(blocks[1..].iter()) {
            self.add_edge(curr, next);
        }
    }

    fn add_current_block(&mut self) -> BlockId {
        self.current_block = self.add_block();
        self.current_block
    }

    fn add_block(&mut self) -> BlockId {
        self.method.body.add_block()
    }

    fn add_blocks(&mut self, amount: usize) -> Vec<BlockId> {
        repeat_with(|| self.add_block()).take(amount).collect()
    }

    fn block_mut(&mut self, index: BlockId) -> &mut Block {
        self.method.body.block_mut(index)
    }

    fn current_block_mut(&mut self) -> &mut Block {
        let index = self.current_block;

        self.method.body.block_mut(index)
    }

    fn in_connected_block(&self) -> bool {
        self.method.body.is_connected(self.current_block)
    }

    /// Returns the register to use for an output expression (`return` or
    /// `throw`).
    fn output_expression(&mut self, node: hir::Expression) -> RegisterId {
        let loc = self.add_location(node.location().clone());
        let reg = self.expression(node);

        self.check_field_move(reg, loc);

        let typ = self.register_type(reg);

        if typ.is_value_type(self.db()) {
            let force_clone = !typ.is_owned_or_uni(self.db());

            return self.clone_value_type(reg, typ, force_clone, loc);
        }

        if typ.is_owned_or_uni(self.db()) {
            self.mark_register_as_moved(reg);
            self.partially_move_self_if_field(reg);

            if let Some(flag) = self.drop_flags.get(&reg).cloned() {
                self.current_block_mut().false_literal(flag, loc);
            }

            return reg;
        }

        // When returning `self`, a reference to a field, or a local variable
        // that stores a reference, we return a new reference. This is needed
        // because for the first two cases we don't create references
        // immediately as that's redundant. It's needed in the last case so we
        // don't mark variables storing references as moved, preventing them
        // from being used afterwards.
        if self.register_kind(reg).new_reference_on_return() {
            let res = self.new_register(typ);

            self.current_block_mut().reference(res, reg, loc);

            return res;
        }

        reg
    }

    fn check_if_moved(
        &mut self,
        register: RegisterId,
        name: &str,
        location: &SourceLocation,
    ) {
        if self.register_is_available(register) {
            return;
        }

        self.state.diagnostics.moved_variable(
            name,
            self.file(),
            location.clone(),
        );
    }

    fn record_loop_move(&mut self, register: RegisterId, location: LocationId) {
        if self.scope.loop_depth == 0 {
            return;
        }

        match self.register_kind(register) {
            RegisterKind::Variable(_, depth)
                if depth < self.scope.loop_depth => {}
            RegisterKind::Field(_) | RegisterKind::SelfObject => {}
            _ => return,
        }

        let mut scope = Some(&mut self.scope);

        while let Some(current) = scope {
            if current.is_loop() {
                current.moved_in_loop.insert(register, location);
                break;
            }

            scope = current.parent.as_mut();
        }
    }

    fn check_field_move(&mut self, register: RegisterId, location: LocationId) {
        if !self.register_kind(register).is_field() {
            return;
        }

        let stype = self.self_type();

        if !stype.has_destructor(self.db()) {
            return;
        }

        let typ = self.register_type(register);

        if !typ.is_owned_or_uni(self.db()) || typ.is_value_type(self.db()) {
            return;
        }

        let loc = self.mir.location(location).clone();

        self.state.diagnostics.error(
            DiagnosticId::Moved,
            format!(
                "this value can't be moved out of '{}', \
                as it defines a custom destructor",
                format_type(self.db(), self.surrounding_type()),
            ),
            self.file(),
            loc,
        );
    }

    fn receiver_for_moving_method(
        &mut self,
        register: RegisterId,
        location: LocationId,
    ) -> RegisterId {
        let typ = self.register_type(register);

        if typ.is_value_type(self.db()) {
            return self.clone_value_type(register, typ, false, location);
        }

        self.check_field_move(register, location);
        self.mark_register_as_moved(register);
        self.partially_move_self_if_field(register);
        self.record_loop_move(register, location);

        if self.register_kind(register).is_field() {
            self.mark_register_as_partially_moved(self.self_register);
        }

        if let Some(flag) = self.drop_flags.get(&register).cloned() {
            self.current_block_mut().false_literal(flag, location);
        }

        register
    }

    fn input_register(
        &mut self,
        register: RegisterId,
        register_type: TypeRef,
        expected: Option<TypeRef>,
        location: LocationId,
    ) -> RegisterId {
        if register_type.is_permanent(self.db()) {
            return register;
        }

        // Value types are always passed as a new value, whether the receiving
        // argument is owned or a reference.
        //
        // This ensures that if we pass the value to generic code, it can freely
        // add references to it (if the value is boxed), without this affecting
        // our current code (i.e. by said reference outliving the input value).
        if register_type.is_value_type(self.db())
            && !register_type.use_atomic_reference_counting(self.db())
        {
            return self.clone_value_type(
                register,
                register_type,
                true,
                location,
            );
        }

        if register_type.is_owned_or_uni(self.db()) {
            if let Some(exp) = expected {
                // Regular owned values passed to references are implicitly
                // passed as references.
                if !exp.is_owned_or_uni(self.db()) {
                    let typ = register_type.cast_according_to(exp, self.db());
                    let reg = self.new_register(typ);

                    self.mark_register_as_moved(reg);
                    self.current_block_mut().reference(reg, register, location);

                    return reg;
                }
            }

            self.check_field_move(register, location);

            if register_type.is_value_type(self.db()) {
                return self.clone_value_type(
                    register,
                    register_type,
                    false,
                    location,
                );
            }

            self.record_loop_move(register, location);
            self.partially_move_self_if_field(register);
            self.mark_register_as_moved(register);

            if let Some(flag) = self.drop_flags.get(&register).cloned() {
                self.current_block_mut().false_literal(flag, location);
            }

            return register;
        }

        // For reference types we only need to increment if they originate from
        // a variable or field, as regular registers can't be referred to more
        // than once.
        if register_type.use_reference_counting(self.db())
            && !self.register_kind(register).is_regular()
        {
            let reg = self.new_register(register_type);

            self.current_block_mut().reference(reg, register, location);
            self.mark_register_as_moved(reg);

            return reg;
        }

        self.mark_register_as_moved(register);
        register
    }

    fn partially_move_self_if_field(&mut self, register: RegisterId) {
        if !self.register_kind(register).is_field() {
            return;
        }

        self.moved_fields += 1;
        self.mark_register_as_partially_moved(self.self_register);
    }

    fn clone_value_type(
        &mut self,
        source: RegisterId,
        typ: TypeRef,
        force_clone: bool,
        location: LocationId,
    ) -> RegisterId {
        if typ.is_permanent(self.db())
            || (self.register_kind(source).is_regular() && !force_clone)
        {
            self.mark_register_as_moved(source);

            // Value types not bound to any variables/fields don't need to be
            // cloned, as there are no additional references to them.
            return source;
        }

        let reg = self.new_register(typ);
        let class = typ.class_id(self.db()).unwrap();

        if class.is_atomic(self.db()) {
            self.current_block_mut().increment_atomic(source, location);
        }

        self.current_block_mut().move_register(reg, source, location);
        self.mark_register_as_moved(reg);
        reg
    }

    fn enter_scope(&mut self) {
        let mut scope = Scope::regular_scope(&self.scope);

        swap(&mut self.scope, &mut scope);

        self.scope.parent = Some(scope);
    }

    fn enter_call_scope(&mut self) -> bool {
        if self.scope.is_call() {
            // Call chains only introduce a single scope for the outer-most
            // call.
            return false;
        }

        let mut scope = Scope::call_scope(&self.scope);

        swap(&mut self.scope, &mut scope);

        self.scope.parent = Some(scope);

        true
    }

    fn enter_loop_scope(&mut self, next_block: BlockId, break_block: BlockId) {
        let mut scope = Scope::loop_scope(&self.scope, next_block, break_block);

        swap(&mut self.scope, &mut scope);

        self.scope.parent = Some(scope);
    }

    fn exit_scope(&mut self) -> Box<Scope> {
        self.drop_scope_registers();

        if let Some(mut scope) = self.scope.parent.take() {
            swap(&mut scope, &mut self.scope);
            scope
        } else {
            panic!("can't exit from the top-level scope");
        }
    }

    fn exit_call_scope(&mut self, entered: bool, register: RegisterId) {
        if !entered {
            // We perform this check here so one can't unconditionally call this
            // method by accident.
            return;
        }

        // Temporarily mark the register as moved so it won't get dropped when
        // we exit the scope.
        self.mark_register_as_moved(register);
        self.exit_scope();
        self.mark_register_as_available(register);

        // Since the register was created in a child scope, we need to store it
        // in the current scope to ensure it gets dropped at the end of said
        // scope.
        self.scope.created.push(register);
    }

    fn drop_scope_registers(&mut self) {
        if !self.in_connected_block() {
            return;
        }

        let loc = self.last_location();

        for index in (0..self.scope.created.len()).rev() {
            let reg = self.scope.created[index];

            if self.should_drop_register(reg) {
                self.drop_register(reg, loc);
            }
        }
    }

    fn drop_all_registers(&mut self) {
        let loc = self.last_location();
        let mut registers = Vec::new();
        let mut scope = Some(&self.scope);

        while let Some(current) = scope {
            for &reg in current.created.iter().rev() {
                registers.push(reg);
            }

            scope = current.parent.as_ref();
        }

        for reg in registers {
            if self.should_drop_register(reg) {
                self.drop_register(reg, loc);
            }
        }

        let self_reg = self.surrounding_type_register;
        let self_type = self.register_type(self_reg);

        if !self.method.id.is_moving(self.db())
            || self_type.is_permanent(self.db())
        {
            return;
        }

        let fields = self.method.id.fields(self.db());
        let partially_moved = fields.iter().any(|(id, _)| {
            self.field_mapping
                .get(id)
                .cloned()
                .map_or(false, |r| !self.register_is_available(r))
        });

        if partially_moved {
            for (id, _) in &fields {
                let reg = self.field_mapping.get(id).cloned().unwrap();

                if self.register_is_moved(reg) {
                    continue;
                }

                self.drop_field(self_reg, *id, reg, loc);
            }
        }

        match self.register_state(self_reg) {
            RegisterState::PartiallyMoved => {
                self.current_block_mut().drop_without_dropper(self_reg, loc);
            }
            RegisterState::Available | RegisterState::MaybeMoved => {
                self.drop_register(self_reg, loc);
            }
            RegisterState::Moved => {}
        }
    }

    fn drop_loop_registers(&mut self, location: LocationId) {
        let mut registers = Vec::new();
        let mut scope = Some(&self.scope);

        while let Some(current) = scope {
            // We push the registers in reverse order so those created later are
            // dropped first.
            for &reg in current.created.iter().rev() {
                registers.push(reg);
            }

            if current.is_loop() {
                break;
            }

            scope = current.parent.as_ref();
        }

        for reg in registers {
            if self.should_drop_register(reg) {
                self.drop_register(reg, location);
            }
        }
    }

    fn drop_register(&mut self, register: RegisterId, location: LocationId) {
        if self.register_might_be_moved(register) {
            let before_block = self.current_block;
            let drop_block = self.add_block();
            let after_block = self.add_block();
            let drop_flag = self.drop_flags.get(&register).cloned().unwrap();

            self.current_block_mut().branch(
                drop_flag,
                drop_block,
                after_block,
                location,
            );

            self.add_edge(before_block, drop_block);
            self.add_edge(before_block, after_block);
            self.add_edge(drop_block, after_block);

            self.current_block = drop_block;

            self.current_block_mut().false_literal(drop_flag, location);
            self.unconditional_drop_register(register, location);

            self.current_block = after_block;

            // Successor blocks may still try to drop the register as the next
            // successor will have two ancestors (the before and drop blocks),
            // but this is redundant because we just dropped it, so we also mark
            // it as moved here.
            self.mark_register_as_moved(register);
        } else {
            self.unconditional_drop_register(register, location);
        }
    }

    fn unconditional_drop_register(
        &mut self,
        register: RegisterId,
        location: LocationId,
    ) {
        self.current_block_mut().drop(register, location);

        // Move it so we don't end up generating another drop somewhere down the
        // line for this same register.
        self.mark_register_as_moved(register);
    }

    fn drop_field(
        &mut self,
        receiver: RegisterId,
        field: types::FieldId,
        register: RegisterId,
        location: LocationId,
    ) {
        if self.register_might_be_moved(register) {
            let before_block = self.current_block;
            let drop_block = self.add_block();
            let after_block = self.add_block();
            let drop_flag = self.drop_flags.get(&register).cloned().unwrap();

            self.current_block_mut().branch(
                drop_flag,
                drop_block,
                after_block,
                location,
            );

            self.add_edge(before_block, drop_block);
            self.add_edge(before_block, after_block);
            self.add_edge(drop_block, after_block);

            self.current_block = drop_block;

            self.current_block_mut().false_literal(drop_flag, location);
            self.unconditional_drop_field(receiver, field, register, location);

            self.current_block = after_block;
            self.mark_register_as_moved(register);
        } else {
            self.unconditional_drop_field(receiver, field, register, location);
        }
    }

    fn unconditional_drop_field(
        &mut self,
        receiver: RegisterId,
        field: types::FieldId,
        register: RegisterId,
        location: LocationId,
    ) {
        let class = self.register_type(receiver).class_id(self.db()).unwrap();

        self.current_block_mut()
            .get_field(register, receiver, class, field, location);
        self.unconditional_drop_register(register, location);
    }

    fn add_drop_flag(&mut self, register: RegisterId, location: LocationId) {
        let typ = self.register_type(register);

        if typ.use_reference_counting(self.db())
            || typ.is_value_type(self.db())
            || typ.is_permanent(self.db())
        {
            return;
        }

        let flag = self.new_register(TypeRef::boolean());

        self.current_block_mut().true_literal(flag, location);
        self.drop_flags.insert(register, flag);
    }

    fn new_untracked_register(&mut self, value_type: TypeRef) -> RegisterId {
        self.add_register(RegisterKind::Regular, value_type)
    }

    fn new_untracked_match_variable(
        &mut self,
        value_type: TypeRef,
    ) -> RegisterId {
        self.add_register(RegisterKind::MatchVariable, value_type)
    }

    fn new_register(&mut self, value_type: TypeRef) -> RegisterId {
        let id = self.add_register(RegisterKind::Regular, value_type);

        self.scope.created.push(id);
        id
    }

    fn new_variable(&mut self, id: types::VariableId) -> RegisterId {
        let reg = self.new_untracked_variable(id);

        self.scope.created.push(reg);
        reg
    }

    fn new_untracked_variable(&mut self, id: types::VariableId) -> RegisterId {
        let typ = id.value_type(self.db());
        let depth = self.scope.depth;

        self.add_register(RegisterKind::Variable(id, depth), typ)
    }

    fn match_binding_registers(
        &mut self,
        ids: Vec<types::VariableId>,
    ) -> Vec<RegisterId> {
        ids.into_iter()
            .map(|id| {
                let reg = self.new_untracked_variable(id);

                self.variable_mapping.insert(id, reg);
                reg
            })
            .collect()
    }

    fn new_field(
        &mut self,
        id: types::FieldId,
        value_type: TypeRef,
    ) -> RegisterId {
        // We don't track these registers in a scope, as fields are dropped at
        // the end of the surrounding method, unless they are moved.
        self.add_register(RegisterKind::Field(id), value_type)
    }

    fn new_self(&mut self, value_type: TypeRef) -> RegisterId {
        let id = self.add_register(RegisterKind::SelfObject, value_type);

        self.scope.created.push(id);
        id
    }

    fn add_register(
        &mut self,
        kind: RegisterKind,
        value_type: TypeRef,
    ) -> RegisterId {
        let id = self.method.registers.alloc(value_type);
        let block = self.current_block;

        self.register_kinds.push(kind);
        self.register_states.set(block, id, RegisterState::Available);
        id
    }

    fn field_register(
        &mut self,
        id: types::FieldId,
        value_type: TypeRef,
        location: LocationId,
    ) -> RegisterId {
        if let Some(reg) = self.field_mapping.get(&id).cloned() {
            return reg;
        }

        let val_reg = self.new_field(id, value_type);

        self.add_drop_flag(val_reg, location);
        self.field_mapping.insert(id, val_reg);
        val_reg
    }

    fn register_type(&self, register: RegisterId) -> TypeRef {
        self.method.registers.value_type(register)
    }

    fn register_kind(&self, register: RegisterId) -> RegisterKind {
        self.register_kinds[register.0 as usize]
    }

    fn register_is_available(&mut self, register: RegisterId) -> bool {
        self.register_state(register) == RegisterState::Available
    }

    fn register_is_moved(&mut self, register: RegisterId) -> bool {
        self.register_state(register) == RegisterState::Moved
    }

    fn register_might_be_moved(&mut self, register: RegisterId) -> bool {
        self.register_state(register) == RegisterState::MaybeMoved
    }

    fn should_drop_register(&mut self, register: RegisterId) -> bool {
        if self.register_is_moved(register)
            || self.register_type(register).is_permanent(self.db())
        {
            return false;
        }

        matches!(
            self.register_kind(register),
            RegisterKind::Regular | RegisterKind::Variable(_, _)
        )
    }

    /// Computes the state for a given register.
    ///
    /// The state may be inherited from the predecessors of the current block.
    /// If a state is available in multiple predecessors, we union the state
    /// into a new state. Take for example this graph:
    ///
    ///     +---+     +---+
    ///     | A |     | B |
    ///     +---+     +---+
    ///       |         |
    ///       +----+----+
    ///            |
    ///            V
    ///          +---+
    ///          | C |
    ///          +---+
    ///
    /// Given a variable used in block C that also exists in block A and B, its
    /// state could be one of the following:
    ///
    /// - available: if in A and B it's also available
    /// - moved: if it's moved in both A and B
    /// - maybe moved: if it's moved in either A or B, while still available in
    ///   the other (or if it's already "maybe moved" in either)
    fn register_state(&mut self, register: RegisterId) -> RegisterState {
        let block = self.current_block;

        if let Some(state) = self.register_states.get(block, register) {
            return state;
        }

        let mut stack = self.method.body.predecessors(block);
        let mut visited = HashSet::new();
        let mut final_state = RegisterState::Available;
        let mut initial = true;

        visited.insert(block);

        while let Some(block) = stack.pop() {
            visited.insert(block);

            if let Some(state) = self.register_states.get(block, register) {
                match final_state {
                    RegisterState::Available if initial => {
                        final_state = state;
                        initial = false;
                    }
                    // We can't transition out of this state, so we don't need
                    // to process new blocks.
                    RegisterState::MaybeMoved => break,
                    RegisterState::Moved
                    | RegisterState::Available
                    | RegisterState::PartiallyMoved => {
                        if final_state != state {
                            final_state = RegisterState::MaybeMoved;
                        }
                    }
                }

                // No need to visit the predecessors of this block.
                continue;
            }

            for block in self.method.body.predecessors(block) {
                if !visited.contains(&block) {
                    stack.push(block);
                }
            }
        }

        // This is an indicationg we're trying to get a register's state, but
        // without first connecting all basic blocks properly.
        debug_assert!(
            !initial,
            "missing state for register r{} in block b{} (method {:?} in {:?})",
            register.0,
            block.0,
            self.method.id.name(self.db()),
            self.file()
        );

        // We copy over the state so we only need to walk the predecessors once
        // for a certain register.
        self.register_states.set(self.current_block, register, final_state);
        final_state
    }

    fn mark_register_as_partially_moved(&mut self, register: RegisterId) {
        self.update_register_state(register, RegisterState::PartiallyMoved);
    }

    fn mark_register_as_moved(&mut self, register: RegisterId) {
        self.update_register_state(register, RegisterState::Moved);
    }

    fn mark_register_as_available(&mut self, register: RegisterId) {
        self.update_register_state(register, RegisterState::Available);
    }

    fn update_register_state(
        &mut self,
        register: RegisterId,
        state: RegisterState,
    ) {
        self.register_states.set(self.current_block, register, state);
    }

    fn add_location(&mut self, range: SourceLocation) -> LocationId {
        self.mir.add_location(range)
    }

    fn last_location(&self) -> LocationId {
        self.mir.last_location().unwrap()
    }

    fn db(&self) -> &types::Database {
        &self.state.db
    }

    fn db_mut(&mut self) -> &mut types::Database {
        &mut self.state.db
    }

    fn file(&self) -> PathBuf {
        self.module.file(&self.state.db)
    }

    fn self_type(&self) -> types::TypeId {
        self.method.id.receiver_id(self.db())
    }

    fn surrounding_type(&self) -> TypeRef {
        self.register_type(self.surrounding_type_register)
    }

    fn in_closure(&self) -> bool {
        self.self_register != self.surrounding_type_register
    }

    fn warn_unreachable(&mut self, location: &SourceLocation) {
        self.check_for_unused_variables();
        self.state.diagnostics.unreachable(self.file(), location.clone());
    }

    fn get_constant(
        &mut self,
        register: RegisterId,
        id: ConstantId,
        location: LocationId,
    ) {
        self.current_block_mut().get_constant(register, id, location);

        // We don't need to handle Array here as it's exposed through a
        // reference, and we never drop the underlying owned value.
        if id.value_type(self.db()).is_string(self.db()) {
            self.current_block_mut().increment_atomic(register, location);
        }
    }

    fn permanent_string(
        &mut self,
        register: RegisterId,
        value: String,
        block: BlockId,
        location: LocationId,
    ) {
        self.block_mut(block).string_literal(register, value, location);

        // This ensures that when the last reference to a string literal goes
        // out of scope, the reference count remains 1, ensuring we don't
        // accidentally drop a permanent string that may be referred to again
        // later.
        self.block_mut(block).increment_atomic(register, location);
    }

    fn mark_variable_as_used(&mut self, id: types::VariableId) {
        self.used_variables.insert(id);
    }

    fn check_for_unused_variables(&mut self) {
        // If dependencies use unused variables there's nothing a project itself
        // can do about it, as changes to ./dep are lost the next time a sync is
        // run. As such, we don't emit unused warnings for dependencies.
        if self.file().starts_with(&self.state.config.dependencies) {
            return;
        }

        let unused = self
            .variable_mapping
            .keys()
            .filter(|&id| {
                !id.name(self.db()).starts_with('_')
                    && !self.used_variables.contains(id)
            })
            .collect::<Vec<_>>();

        for id in unused {
            let name = id.name(self.db()).clone();
            let var_loc = id.location(self.db());
            let src_loc = SourceLocation::new(
                var_loc.line..=var_loc.line,
                var_loc.start_column..=var_loc.end_column,
            );

            self.state.diagnostics.unused_variable(&name, self.file(), src_loc);
        }
    }
}

/// A compiler pass that cleans up basic blocks.
///
/// This pass does the following:
///
/// 1. Empty basic blocks are removed.
/// 2. Basic blocks that implicitly flow into another block are updated to end
///    with a goto to said block.
///
/// These changes make it easier to visualize the MIR code, and result in a
/// smaller IR to pass to LLVM.
pub(crate) fn clean_up_basic_blocks(mir: &mut Mir) {
    for method in mir.methods.values_mut() {
        let blocks = &method.body.blocks;
        let mut new_blocks = Vec::new();
        let mut id_map = vec![BlockId(0); blocks.len()];
        let mut valid = Vec::with_capacity(blocks.len());

        for (index, block) in blocks.iter().enumerate() {
            if block.instructions.is_empty()
                || !method.body.is_connected(BlockId(index))
            {
                // Empty and unreachable blocks are useless, so we get rid of
                // them here.
                continue;
            }

            id_map[index] = BlockId(new_blocks.len());
            valid.push((index, block));
            new_blocks.push(Block::new());
        }

        for (index, block) in valid {
            let block_id = id_map[index];

            new_blocks[block_id.0].instructions = block.instructions.clone();

            let successors =
                match new_blocks[block_id.0].instructions.last_mut().unwrap() {
                    Instruction::Branch(ins) => {
                        let ok = id_map[find_successor(blocks, ins.if_true).0];
                        let err =
                            id_map[find_successor(blocks, ins.if_false).0];

                        ins.if_true = ok;
                        ins.if_false = err;

                        vec![ok, err]
                    }
                    Instruction::DecrementAtomic(ins) => {
                        let ok = id_map[find_successor(blocks, ins.if_true).0];
                        let err =
                            id_map[find_successor(blocks, ins.if_false).0];

                        ins.if_true = ok;
                        ins.if_false = err;

                        vec![ok, err]
                    }
                    Instruction::Switch(ins) => {
                        for index in 0..ins.blocks.len() {
                            let old_id = ins.blocks[index];

                            ins.blocks[index] =
                                id_map[find_successor(blocks, old_id).0];
                        }

                        ins.blocks.clone()
                    }
                    Instruction::Goto(ins) => {
                        ins.block = id_map[find_successor(blocks, ins.block).0];

                        vec![ins.block]
                    }
                    _ if block.successors.len() == 1 => {
                        let new_id = id_map
                            [find_successor(blocks, block.successors[0]).0];

                        let location =
                            block.instructions.last().unwrap().location();

                        new_blocks[block_id.0].instructions.push(
                            Instruction::Goto(Box::new(Goto {
                                block: new_id,
                                location,
                            })),
                        );

                        vec![new_id]
                    }
                    _ => {
                        // A block without an exit can only have one successor,
                        // and we handle that case above. This means this code
                        // only runs for blocks without successors, for which no
                        // extra work is necessary.
                        continue;
                    }
                };

            for &succ in &successors {
                new_blocks[succ.0].predecessors.push(block_id);
            }

            new_blocks[block_id.0].successors = successors;
        }

        // The first block (ID 0) may be empty, depending on the MIR that was
        // generated. The above code will remove that block, meaning we have to
        // determine a new start block. Since this only happens when the block
        // is empty, and empty blocks only have one successor, we just make the
        // successor the new starting block.
        let old_start = &blocks[method.body.start_id.0];

        if old_start.instructions.is_empty() {
            method.body.start_id =
                id_map[find_successor(blocks, old_start.successors[0]).0];
        }

        method.body.blocks = new_blocks;
    }
}

fn find_successor(blocks: &[Block], old_id: BlockId) -> BlockId {
    let mut id = old_id;

    loop {
        let block = &blocks[id.0];

        if !block.instructions.is_empty() {
            break;
        }

        id = block.successors[0];
    }

    id
}
