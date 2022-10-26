//! A mid-level graph IR.
//!
//! MIR is used for various optimisations, analysing moves of values, compiling
//! pattern matching into decision trees, and more.
use ast::source_location::SourceLocation;
use bytecode::{BuiltinFunction, Opcode};
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::hash::{Hash, Hasher};
use types::collections::IndexMap;

/// The number of reductions to perform after calling a method.
const CALL_COST: u16 = 1;

pub(crate) mod passes;
pub(crate) mod pattern_matching;
pub(crate) mod printer;

fn join(values: &[RegisterId]) -> String {
    values.iter().map(|v| format!("r{}", v.0)).collect::<Vec<_>>().join(", ")
}

#[derive(Clone)]
pub(crate) struct Registers {
    values: Vec<Register>,
}

impl Registers {
    pub(crate) fn new() -> Self {
        Self { values: Vec::new() }
    }

    pub(crate) fn alloc(&mut self, value_type: types::TypeRef) -> RegisterId {
        let id = self.values.len();

        self.values.push(Register { value_type });
        RegisterId(id)
    }

    pub(crate) fn get(&self, register: RegisterId) -> &Register {
        &self.values[register.0]
    }

    pub(crate) fn value_type(&self, register: RegisterId) -> types::TypeRef {
        self.get(register).value_type
    }

    pub(crate) fn len(&self) -> usize {
        self.values.len()
    }
}

/// A directed control-flow graph.
#[derive(Clone)]
pub(crate) struct Graph {
    pub(crate) blocks: Vec<Block>,
    pub(crate) start_id: BlockId,
}

impl Graph {
    pub(crate) fn new() -> Self {
        Self { blocks: Vec::new(), start_id: BlockId(0) }
    }

    pub(crate) fn add_start_block(&mut self) -> BlockId {
        let id = BlockId(self.blocks.len());

        self.blocks.push(Block::new());
        id
    }

    pub(crate) fn add_block(&mut self) -> BlockId {
        let id = BlockId(self.blocks.len());

        self.blocks.push(Block::new());
        id
    }

    pub(crate) fn block_mut(&mut self, index: BlockId) -> &mut Block {
        self.blocks.get_mut(index.0).unwrap()
    }

    pub(crate) fn add_edge(&mut self, source: BlockId, target: BlockId) {
        self.blocks[target.0].predecessors.push(source);
        self.blocks[source.0].successors.push(target);
    }

    pub(crate) fn is_connected(&self, block: BlockId) -> bool {
        block.0 == 0 || !self.blocks[block.0].predecessors.is_empty()
    }

    pub(crate) fn predecessors(&self, block: BlockId) -> Vec<BlockId> {
        self.blocks[block.0].predecessors.clone()
    }

    pub(crate) fn successors(&self, block: BlockId) -> Vec<BlockId> {
        self.blocks[block.0].successors.clone()
    }

    pub(crate) fn remove_predecessor(
        &mut self,
        block: BlockId,
        remove: BlockId,
    ) {
        self.blocks[block.0].predecessors.retain(|&v| v != remove);
    }

    pub(crate) fn reachable(&self) -> HashSet<BlockId> {
        let mut reachable = HashSet::new();

        // The start block is always implicitly reachable.
        reachable.insert(self.start_id);

        for (index, block) in self.blocks.iter().enumerate() {
            if !block.predecessors.is_empty() {
                reachable.insert(BlockId(index));
            }
        }

        reachable
    }
}

/// The ID/index to a basic block within a method.
#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq)]
pub(crate) struct BlockId(pub(crate) usize);

/// A basic block in a control flow graph.
#[derive(Clone)]
pub(crate) struct Block {
    /// The MIR instructions in this block.
    pub(crate) instructions: Vec<Instruction>,

    /// All the successors of this block.
    pub(crate) successors: Vec<BlockId>,

    /// All the predecessors of this block.
    pub(crate) predecessors: Vec<BlockId>,
}

impl Block {
    pub(crate) fn new() -> Self {
        Self {
            instructions: Vec::new(),
            successors: Vec::new(),
            predecessors: Vec::new(),
        }
    }

    pub(crate) fn goto(&mut self, block: BlockId, location: LocationId) {
        self.instructions
            .push(Instruction::Goto(Box::new(Goto { block, location })));
    }

    pub(crate) fn branch(
        &mut self,
        condition: RegisterId,
        if_true: BlockId,
        if_false: BlockId,
        location: LocationId,
    ) {
        self.instructions.push(Instruction::Branch(Box::new(Branch {
            condition,
            if_true,
            if_false,
            location,
        })));
    }

    pub(crate) fn branch_result(
        &mut self,
        ok: BlockId,
        error: BlockId,
        location: LocationId,
    ) {
        self.instructions.push(Instruction::BranchResult(Box::new(
            BranchResult { ok, error, location },
        )));
    }

    pub(crate) fn jump_table(
        &mut self,
        register: RegisterId,
        blocks: Vec<BlockId>,
        location: LocationId,
    ) {
        self.instructions.push(Instruction::JumpTable(Box::new(JumpTable {
            register,
            blocks,
            location,
        })));
    }

    pub(crate) fn return_value(
        &mut self,
        register: RegisterId,
        location: LocationId,
    ) {
        self.instructions
            .push(Instruction::Return(Box::new(Return { register, location })));
    }

    pub(crate) fn throw_value(
        &mut self,
        register: RegisterId,
        location: LocationId,
    ) {
        self.instructions
            .push(Instruction::Throw(Box::new(Throw { register, location })));
    }

    pub(crate) fn return_async_value(
        &mut self,
        register: RegisterId,
        value: RegisterId,
        location: LocationId,
    ) {
        self.instructions.push(Instruction::ReturnAsync(Box::new(
            ReturnAsync { register, value, location },
        )));
    }

    pub(crate) fn throw_async_value(
        &mut self,
        register: RegisterId,
        value: RegisterId,
        location: LocationId,
    ) {
        self.instructions.push(Instruction::ThrowAsync(Box::new(ThrowAsync {
            register,
            value,
            location,
        })));
    }

    pub(crate) fn finish(&mut self, terminate: bool, location: LocationId) {
        self.instructions.push(Instruction::Finish(Box::new(Finish {
            location,
            terminate,
        })));
    }

    pub(crate) fn nil_literal(
        &mut self,
        register: RegisterId,
        location: LocationId,
    ) {
        self.instructions.push(Instruction::Nil(Box::new(NilLiteral {
            register,
            location,
        })));
    }

    pub(crate) fn false_literal(
        &mut self,
        register: RegisterId,
        location: LocationId,
    ) {
        self.instructions.push(Instruction::False(Box::new(FalseLiteral {
            register,
            location,
        })));
    }

    pub(crate) fn true_literal(
        &mut self,
        register: RegisterId,
        location: LocationId,
    ) {
        self.instructions.push(Instruction::True(Box::new(TrueLiteral {
            register,
            location,
        })));
    }

    pub(crate) fn int_literal(
        &mut self,
        register: RegisterId,
        value: i64,
        location: LocationId,
    ) {
        self.instructions.push(Instruction::Int(Box::new(IntLiteral {
            register,
            value,
            location,
        })));
    }

    pub(crate) fn float_literal(
        &mut self,
        register: RegisterId,
        value: f64,
        location: LocationId,
    ) {
        self.instructions.push(Instruction::Float(Box::new(FloatLiteral {
            register,
            value,
            location,
        })));
    }

    pub(crate) fn string_literal(
        &mut self,
        register: RegisterId,
        value: String,
        location: LocationId,
    ) {
        self.instructions.push(Instruction::String(Box::new(StringLiteral {
            register,
            value,
            location,
        })));
    }

    pub(crate) fn allocate_array(
        &mut self,
        register: RegisterId,
        values: Vec<RegisterId>,
        location: LocationId,
    ) {
        self.instructions.push(Instruction::AllocateArray(Box::new(
            AllocateArray { register, values, location },
        )));
    }

    pub(crate) fn strings(
        &mut self,
        register: RegisterId,
        values: Vec<RegisterId>,
        location: LocationId,
    ) {
        self.instructions.push(Instruction::Strings(Box::new(Strings {
            register,
            values,
            location,
        })));
    }

    pub(crate) fn move_register(
        &mut self,
        target: RegisterId,
        source: RegisterId,
        location: LocationId,
    ) {
        self.instructions.push(Instruction::MoveRegister(Box::new(
            MoveRegister { source, target, location },
        )));
    }

    pub(crate) fn increment(
        &mut self,
        register: RegisterId,
        value: RegisterId,
        location: LocationId,
    ) {
        self.instructions.push(Instruction::Increment(Box::new(Increment {
            register,
            value,
            location,
        })));
    }

    pub(crate) fn decrement(
        &mut self,
        register: RegisterId,
        location: LocationId,
    ) {
        self.instructions.push(Instruction::Decrement(Box::new(Decrement {
            register,
            location,
        })));
    }

    pub(crate) fn decrement_atomic(
        &mut self,
        register: RegisterId,
        value: RegisterId,
        location: LocationId,
    ) {
        self.instructions.push(Instruction::DecrementAtomic(Box::new(
            DecrementAtomic { register, value, location },
        )));
    }

    pub(crate) fn drop(&mut self, register: RegisterId, location: LocationId) {
        self.instructions.push(Instruction::Drop(Box::new(Drop {
            register,
            dropper: true,
            location,
        })));
    }

    pub(crate) fn drop_without_dropper(
        &mut self,
        register: RegisterId,
        location: LocationId,
    ) {
        self.instructions.push(Instruction::Drop(Box::new(Drop {
            register,
            dropper: false,
            location,
        })));
    }

    pub(crate) fn free(&mut self, register: RegisterId, location: LocationId) {
        self.instructions
            .push(Instruction::Free(Box::new(Free { register, location })));
    }

    pub(crate) fn clone(
        &mut self,
        kind: CloneKind,
        register: RegisterId,
        source: RegisterId,
        location: LocationId,
    ) {
        self.instructions.push(Instruction::Clone(Box::new(Clone {
            kind,
            register,
            source,
            location,
        })));
    }

    pub(crate) fn check_refs(
        &mut self,
        register: RegisterId,
        location: LocationId,
    ) {
        self.instructions.push(Instruction::CheckRefs(Box::new(CheckRefs {
            register,
            location,
        })));
    }

    pub(crate) fn ref_kind(
        &mut self,
        register: RegisterId,
        value: RegisterId,
        location: LocationId,
    ) {
        self.instructions.push(Instruction::RefKind(Box::new(RefKind {
            register,
            value,
            location,
        })));
    }

    pub(crate) fn move_result(
        &mut self,
        register: RegisterId,
        location: LocationId,
    ) {
        self.instructions.push(Instruction::MoveResult(Box::new(MoveResult {
            register,
            location,
        })));
    }

    pub(crate) fn call_static(
        &mut self,
        class: types::ClassId,
        method: types::MethodId,
        arguments: Vec<RegisterId>,
        location: LocationId,
    ) {
        self.instructions.push(Instruction::CallStatic(Box::new(CallStatic {
            class,
            method,
            arguments,
            location,
        })));
    }

    pub(crate) fn call_virtual(
        &mut self,
        receiver: RegisterId,
        method: types::MethodId,
        arguments: Vec<RegisterId>,
        location: LocationId,
    ) {
        self.instructions.push(Instruction::CallVirtual(Box::new(
            CallVirtual { receiver, method, arguments, location },
        )));
    }

    pub(crate) fn call_dynamic(
        &mut self,
        receiver: RegisterId,
        method: types::MethodId,
        arguments: Vec<RegisterId>,
        location: LocationId,
    ) {
        self.instructions.push(Instruction::CallDynamic(Box::new(
            CallDynamic { receiver, method, arguments, location },
        )));
    }

    pub(crate) fn call_closure(
        &mut self,
        receiver: RegisterId,
        arguments: Vec<RegisterId>,
        location: LocationId,
    ) {
        self.instructions.push(Instruction::CallClosure(Box::new(
            CallClosure { receiver, arguments, location },
        )));
    }

    pub(crate) fn call_dropper(
        &mut self,
        receiver: RegisterId,
        location: LocationId,
    ) {
        self.instructions.push(Instruction::CallDropper(Box::new(
            CallDropper { receiver, location },
        )));
    }

    pub(crate) fn call_builtin(
        &mut self,
        id: BuiltinFunction,
        arguments: Vec<RegisterId>,
        location: LocationId,
    ) {
        self.instructions.push(Instruction::CallBuiltin(Box::new(
            CallBuiltin { id, arguments, location },
        )));
    }

    pub(crate) fn raw_instruction(
        &mut self,
        opcode: Opcode,
        arguments: Vec<RegisterId>,
        location: LocationId,
    ) {
        self.instructions.push(Instruction::RawInstruction(Box::new(
            RawInstruction { opcode, arguments, location },
        )));
    }

    pub(crate) fn send_and_wait(
        &mut self,
        receiver: RegisterId,
        method: types::MethodId,
        arguments: Vec<RegisterId>,
        location: LocationId,
    ) {
        self.instructions.push(Instruction::Send(Box::new(Send {
            receiver,
            method,
            arguments,
            location,
            wait: true,
        })));
    }

    pub(crate) fn send_and_forget(
        &mut self,
        receiver: RegisterId,
        method: types::MethodId,
        arguments: Vec<RegisterId>,
        location: LocationId,
    ) {
        self.instructions.push(Instruction::Send(Box::new(Send {
            receiver,
            method,
            arguments,
            location,
            wait: false,
        })));
    }

    pub(crate) fn send_async(
        &mut self,
        register: RegisterId,
        receiver: RegisterId,
        method: types::MethodId,
        arguments: Vec<RegisterId>,
        location: LocationId,
    ) {
        self.instructions.push(Instruction::SendAsync(Box::new(SendAsync {
            register,
            receiver,
            method,
            arguments,
            location,
        })));
    }

    pub(crate) fn get_field(
        &mut self,
        register: RegisterId,
        receiver: RegisterId,
        field: types::FieldId,
        location: LocationId,
    ) {
        self.instructions.push(Instruction::GetField(Box::new(GetField {
            register,
            receiver,
            field,
            location,
        })));
    }

    pub(crate) fn set_field(
        &mut self,
        receiver: RegisterId,
        field: types::FieldId,
        value: RegisterId,
        location: LocationId,
    ) {
        self.instructions.push(Instruction::SetField(Box::new(SetField {
            receiver,
            value,
            field,
            location,
        })));
    }

    pub(crate) fn allocate(
        &mut self,
        register: RegisterId,
        class: types::ClassId,
        location: LocationId,
    ) {
        self.instructions.push(Instruction::Allocate(Box::new(Allocate {
            register,
            class,
            location,
        })));
    }

    pub(crate) fn get_constant(
        &mut self,
        register: RegisterId,
        id: types::ConstantId,
        location: LocationId,
    ) {
        self.instructions.push(Instruction::GetConstant(Box::new(
            GetConstant { register, id, location },
        )));
    }

    pub(crate) fn string_eq(
        &mut self,
        register: RegisterId,
        left: RegisterId,
        right: RegisterId,
        location: LocationId,
    ) {
        self.instructions.push(Instruction::StringEq(Box::new(StringEq {
            register,
            left,
            right,
            location,
        })))
    }

    pub(crate) fn int_eq(
        &mut self,
        register: RegisterId,
        left: RegisterId,
        right: RegisterId,
        location: LocationId,
    ) {
        self.instructions.push(Instruction::IntEq(Box::new(IntEq {
            register,
            left,
            right,
            location,
        })))
    }

    pub(crate) fn reduce(&mut self, amount: u16, location: LocationId) {
        self.instructions
            .push(Instruction::Reduce(Box::new(Reduce { amount, location })))
    }

    pub(crate) fn reduce_call(&mut self, location: LocationId) {
        self.reduce(CALL_COST, location);
    }
}

#[derive(Clone, Debug)]
pub(crate) enum Constant {
    Int(i64),
    Float(f64),
    String(String),
    Array(Vec<Constant>),
}

impl PartialEq for Constant {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Constant::Int(a), Constant::Int(b)) => a == b,
            // When comparing float constants we _shouldn't_ treat -0.0 and 0.0
            // as being the same constants, as this could mess up the generated
            // code. For example, if we treated them as the same the expression
            // `-0.0.to_string` could randomly evaluate to `"0.0"`, which isn't
            // correct.
            (Constant::Float(a), Constant::Float(b))
                if a.is_sign_positive() == b.is_sign_positive() =>
            {
                a == b
            }
            (Constant::String(a), Constant::String(b)) => a == b,
            (Constant::Array(a), Constant::Array(b)) => a == b,
            _ => false,
        }
    }
}

impl Eq for Constant {}

impl Hash for Constant {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self {
            Constant::Int(v) => v.hash(state),
            Constant::Float(v) => v.to_bits().hash(state),
            Constant::String(v) => v.hash(state),
            Constant::Array(v) => v.hash(state),
        }
    }
}

impl fmt::Display for Constant {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Int(v) => write!(f, "{}", v),
            Self::Float(v) => write!(f, "{}", v),
            Self::String(v) => write!(f, "{:?}", v),
            Self::Array(v) => write!(f, "{:?}", v),
        }
    }
}

/// A MIR register/temporary variable.
///
/// Registers may be introduced through user-defined local variables,
/// sub-expressions, or just because the compiler feels like it. In other words,
/// they're not always directly tied to variables in the source code.
#[derive(Clone)]
pub(crate) struct Register {
    pub(crate) value_type: types::TypeRef,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub(crate) struct RegisterId(pub(crate) usize);

#[derive(Clone)]
pub(crate) struct Branch {
    pub(crate) condition: RegisterId,
    pub(crate) if_true: BlockId,
    pub(crate) if_false: BlockId,
    pub(crate) location: LocationId,
}

/// A jump table/switch instruction.
///
/// This instruction expects a list of blocks to jump to for their corresponding
/// indexes/values. As such, it currently only supports monotonically increasing
/// values that start at zero.
#[derive(Clone)]
pub(crate) struct JumpTable {
    pub(crate) register: RegisterId,
    pub(crate) blocks: Vec<BlockId>,
    pub(crate) location: LocationId,
}

#[derive(Clone)]
pub(crate) struct BranchResult {
    pub(crate) ok: BlockId,
    pub(crate) error: BlockId,
    pub(crate) location: LocationId,
}

#[derive(Clone)]
pub(crate) struct Goto {
    pub(crate) block: BlockId,
    pub(crate) location: LocationId,
}

/// Moves a value from one register to another.
#[derive(Clone)]
pub(crate) struct MoveRegister {
    pub(crate) source: RegisterId,
    pub(crate) target: RegisterId,
    pub(crate) location: LocationId,
}

/// Checks if the reference count of a register is zero.
///
/// If the value in the register has any references left, a runtime panic is
/// produced.
#[derive(Clone)]
pub(crate) struct CheckRefs {
    pub(crate) register: RegisterId,
    pub(crate) location: LocationId,
}

/// Returns the kind of reference we're dealing with.
///
/// The following values may be produced:
///
/// - `0`: owned
/// - `1`: a regular reference
/// - `2`: an atomic type (either a reference or owned)
#[derive(Clone)]
pub(crate) struct RefKind {
    pub(crate) register: RegisterId,
    pub(crate) value: RegisterId,
    pub(crate) location: LocationId,
}

/// Drops a value according to its type.
///
/// This instruction is essentially a macro that expands into one of the
/// following (given the instruction `drop(value)`):
///
/// For an owned value, it checks the reference count and calls the value's
/// dropper method:
///
///     check_refs(value)
///     value.$dropper()
///
/// For a regular reference, it decrements the reference count:
///
///     decrement(value)
///
/// For a value that uses atomic reference counting, it decrements the count.
/// If the new count is zero, the dropper method is called:
///
///     if decrement_atomic(value) {
///       value.$dropper()
///     }
///
/// For a process it does the same as an atomically reference counted value,
/// except the dropper method is scheduled asynchronously:
///
///     if decrement_atomic(value) {
///       async value.$dropper()
///     }
///
/// If `dropper` is set to `false`, the dropper method isn't called for a value
/// no longer in use.
#[derive(Clone)]
pub(crate) struct Drop {
    pub(crate) register: RegisterId,
    pub(crate) dropper: bool,
    pub(crate) location: LocationId,
}

#[derive(Clone)]
pub(crate) struct CallDropper {
    pub(crate) receiver: RegisterId,
    pub(crate) location: LocationId,
}

#[derive(Clone)]
pub(crate) struct Free {
    pub(crate) register: RegisterId,
    pub(crate) location: LocationId,
}

#[derive(Clone, Eq, PartialEq, Debug)]
pub(crate) enum CloneKind {
    Float,
    Int,
    Process,
    String,
    Other,
}

/// Clones a value type.
///
/// This is a dedicated instruction in MIR so it's a bit easier to optimise
/// (e.g. removing redundant clones) compared to regular method calls.
#[derive(Clone)]
pub(crate) struct Clone {
    pub(crate) kind: CloneKind,
    pub(crate) register: RegisterId,
    pub(crate) source: RegisterId,
    pub(crate) location: LocationId,
}

/// Increments the reference count of a value.
///
/// For regular values this uses regular reference counting. For values that use
/// atomic reference counting (e.g. processes), this instruction uses atomic
/// reference counting.
///
/// If used on a type parameter, this instruction compiles down to the
/// equivalent of:
///
///     if atomic(value) {
///       increment_atomic(value)
///     } else {
///       increment(value)
///     }
#[derive(Clone)]
pub(crate) struct Increment {
    pub(crate) register: RegisterId,
    pub(crate) value: RegisterId,
    pub(crate) location: LocationId,
}

#[derive(Clone)]
pub(crate) struct Decrement {
    pub(crate) register: RegisterId,
    pub(crate) location: LocationId,
}

#[derive(Clone)]
pub(crate) struct DecrementAtomic {
    pub(crate) register: RegisterId,
    pub(crate) value: RegisterId,
    pub(crate) location: LocationId,
}

/// Moves the result of the last method call into a register.
#[derive(Clone)]
pub(crate) struct MoveResult {
    pub(crate) register: RegisterId,
    pub(crate) location: LocationId,
}

#[derive(Clone)]
pub(crate) struct TrueLiteral {
    pub(crate) register: RegisterId,
    pub(crate) location: LocationId,
}

#[derive(Clone)]
pub(crate) struct FalseLiteral {
    pub(crate) register: RegisterId,
    pub(crate) location: LocationId,
}

#[derive(Clone)]
pub(crate) struct NilLiteral {
    pub(crate) register: RegisterId,
    pub(crate) location: LocationId,
}

#[derive(Clone)]
pub(crate) struct AllocateArray {
    pub(crate) register: RegisterId,
    pub(crate) values: Vec<RegisterId>,
    pub(crate) location: LocationId,
}

#[derive(Clone)]
pub(crate) struct Return {
    pub(crate) register: RegisterId,
    pub(crate) location: LocationId,
}

#[derive(Clone)]
pub(crate) struct Throw {
    pub(crate) register: RegisterId,
    pub(crate) location: LocationId,
}

#[derive(Clone)]
pub(crate) struct ReturnAsync {
    pub(crate) register: RegisterId,
    pub(crate) value: RegisterId,
    pub(crate) location: LocationId,
}

#[derive(Clone)]
pub(crate) struct ThrowAsync {
    pub(crate) register: RegisterId,
    pub(crate) value: RegisterId,
    pub(crate) location: LocationId,
}

#[derive(Clone)]
pub(crate) struct IntLiteral {
    pub(crate) register: RegisterId,
    pub(crate) value: i64,
    pub(crate) location: LocationId,
}

#[derive(Clone)]
pub(crate) struct FloatLiteral {
    pub(crate) register: RegisterId,
    pub(crate) value: f64,
    pub(crate) location: LocationId,
}

#[derive(Clone)]
pub(crate) struct StringLiteral {
    pub(crate) register: RegisterId,
    pub(crate) value: String,
    pub(crate) location: LocationId,
}

#[derive(Clone)]
pub(crate) struct Strings {
    pub(crate) register: RegisterId,
    pub(crate) values: Vec<RegisterId>,
    pub(crate) location: LocationId,
}

#[derive(Clone)]
pub(crate) struct CallStatic {
    pub(crate) class: types::ClassId,
    pub(crate) method: types::MethodId,
    pub(crate) arguments: Vec<RegisterId>,
    pub(crate) location: LocationId,
}

#[derive(Clone)]
pub(crate) struct CallVirtual {
    pub(crate) receiver: RegisterId,
    pub(crate) method: types::MethodId,
    pub(crate) arguments: Vec<RegisterId>,
    pub(crate) location: LocationId,
}

#[derive(Clone)]
pub(crate) struct CallDynamic {
    pub(crate) receiver: RegisterId,
    pub(crate) method: types::MethodId,
    pub(crate) arguments: Vec<RegisterId>,
    pub(crate) location: LocationId,
}

#[derive(Clone)]
pub(crate) struct CallClosure {
    pub(crate) receiver: RegisterId,
    pub(crate) arguments: Vec<RegisterId>,
    pub(crate) location: LocationId,
}

#[derive(Clone)]
pub(crate) struct CallBuiltin {
    pub(crate) id: BuiltinFunction,
    pub(crate) arguments: Vec<RegisterId>,
    pub(crate) location: LocationId,
}

#[derive(Clone)]
pub(crate) struct RawInstruction {
    pub(crate) opcode: Opcode,
    pub(crate) arguments: Vec<RegisterId>,
    pub(crate) location: LocationId,
}

#[derive(Clone)]
pub(crate) struct Send {
    pub(crate) receiver: RegisterId,
    pub(crate) method: types::MethodId,
    pub(crate) arguments: Vec<RegisterId>,
    pub(crate) location: LocationId,
    pub(crate) wait: bool,
}

#[derive(Clone)]
pub(crate) struct SendAsync {
    pub(crate) register: RegisterId,
    pub(crate) receiver: RegisterId,
    pub(crate) method: types::MethodId,
    pub(crate) arguments: Vec<RegisterId>,
    pub(crate) location: LocationId,
}

#[derive(Clone)]
pub(crate) struct GetField {
    pub(crate) register: RegisterId,
    pub(crate) receiver: RegisterId,
    pub(crate) field: types::FieldId,
    pub(crate) location: LocationId,
}

#[derive(Clone)]
pub(crate) struct SetField {
    pub(crate) receiver: RegisterId,
    pub(crate) value: RegisterId,
    pub(crate) field: types::FieldId,
    pub(crate) location: LocationId,
}

#[derive(Clone)]
pub(crate) struct GetConstant {
    pub(crate) register: RegisterId,
    pub(crate) id: types::ConstantId,
    pub(crate) location: LocationId,
}

#[derive(Clone)]
pub(crate) struct Allocate {
    pub(crate) register: RegisterId,
    pub(crate) class: types::ClassId,
    pub(crate) location: LocationId,
}

#[derive(Clone)]
pub(crate) struct IntEq {
    pub(crate) register: RegisterId,
    pub(crate) left: RegisterId,
    pub(crate) right: RegisterId,
    pub(crate) location: LocationId,
}

#[derive(Clone)]
pub(crate) struct StringEq {
    pub(crate) register: RegisterId,
    pub(crate) left: RegisterId,
    pub(crate) right: RegisterId,
    pub(crate) location: LocationId,
}

#[derive(Clone)]
pub(crate) struct Reduce {
    pub(crate) amount: u16,
    pub(crate) location: LocationId,
}

#[derive(Clone)]
pub(crate) struct Finish {
    pub(crate) terminate: bool,
    pub(crate) location: LocationId,
}

/// A MIR instruction.
///
/// When adding a new instruction that acts as an exit for a basic block, make
/// sure to also update the compiler pass that removes empty basic blocks.
#[derive(Clone)]
pub(crate) enum Instruction {
    AllocateArray(Box<AllocateArray>),
    Branch(Box<Branch>),
    BranchResult(Box<BranchResult>),
    JumpTable(Box<JumpTable>),
    False(Box<FalseLiteral>),
    Float(Box<FloatLiteral>),
    Goto(Box<Goto>),
    Int(Box<IntLiteral>),
    MoveRegister(Box<MoveRegister>),
    MoveResult(Box<MoveResult>),
    Nil(Box<NilLiteral>),
    Return(Box<Return>),
    Throw(Box<Throw>),
    ReturnAsync(Box<ReturnAsync>),
    ThrowAsync(Box<ThrowAsync>),
    String(Box<StringLiteral>),
    True(Box<TrueLiteral>),
    Strings(Box<Strings>),
    CallStatic(Box<CallStatic>),
    CallVirtual(Box<CallVirtual>),
    CallDynamic(Box<CallDynamic>),
    CallClosure(Box<CallClosure>),
    CallDropper(Box<CallDropper>),
    CallBuiltin(Box<CallBuiltin>),
    Send(Box<Send>),
    SendAsync(Box<SendAsync>),
    GetField(Box<GetField>),
    SetField(Box<SetField>),
    CheckRefs(Box<CheckRefs>),
    RefKind(Box<RefKind>),
    Drop(Box<Drop>),
    Free(Box<Free>),
    Clone(Box<Clone>),
    Increment(Box<Increment>),
    Decrement(Box<Decrement>),
    DecrementAtomic(Box<DecrementAtomic>),
    RawInstruction(Box<RawInstruction>),
    Allocate(Box<Allocate>),
    GetConstant(Box<GetConstant>),
    IntEq(Box<IntEq>),
    StringEq(Box<StringEq>),
    Reduce(Box<Reduce>),
    Finish(Box<Finish>),
}

impl Instruction {
    fn location(&self) -> LocationId {
        match self {
            Instruction::Branch(ref v) => v.location,
            Instruction::BranchResult(ref v) => v.location,
            Instruction::JumpTable(ref v) => v.location,
            Instruction::False(ref v) => v.location,
            Instruction::True(ref v) => v.location,
            Instruction::Goto(ref v) => v.location,
            Instruction::MoveRegister(ref v) => v.location,
            Instruction::MoveResult(ref v) => v.location,
            Instruction::Return(ref v) => v.location,
            Instruction::Throw(ref v) => v.location,
            Instruction::ReturnAsync(ref v) => v.location,
            Instruction::ThrowAsync(ref v) => v.location,
            Instruction::Nil(ref v) => v.location,
            Instruction::AllocateArray(ref v) => v.location,
            Instruction::Int(ref v) => v.location,
            Instruction::Float(ref v) => v.location,
            Instruction::String(ref v) => v.location,
            Instruction::Strings(ref v) => v.location,
            Instruction::CallStatic(ref v) => v.location,
            Instruction::CallVirtual(ref v) => v.location,
            Instruction::CallDynamic(ref v) => v.location,
            Instruction::CallClosure(ref v) => v.location,
            Instruction::CallDropper(ref v) => v.location,
            Instruction::CallBuiltin(ref v) => v.location,
            Instruction::Send(ref v) => v.location,
            Instruction::SendAsync(ref v) => v.location,
            Instruction::GetField(ref v) => v.location,
            Instruction::SetField(ref v) => v.location,
            Instruction::CheckRefs(ref v) => v.location,
            Instruction::RefKind(ref v) => v.location,
            Instruction::Drop(ref v) => v.location,
            Instruction::Free(ref v) => v.location,
            Instruction::Clone(ref v) => v.location,
            Instruction::Increment(ref v) => v.location,
            Instruction::Decrement(ref v) => v.location,
            Instruction::DecrementAtomic(ref v) => v.location,
            Instruction::RawInstruction(ref v) => v.location,
            Instruction::Allocate(ref v) => v.location,
            Instruction::GetConstant(ref v) => v.location,
            Instruction::IntEq(ref v) => v.location,
            Instruction::StringEq(ref v) => v.location,
            Instruction::Reduce(ref v) => v.location,
            Instruction::Finish(ref v) => v.location,
        }
    }

    fn format(&self, db: &types::Database) -> String {
        match self {
            Instruction::Branch(ref v) => {
                format!(
                    "branch r{}, true = b{}, false = b{}",
                    v.condition.0, v.if_true.0, v.if_false.0
                )
            }
            Instruction::BranchResult(ref v) => {
                format!(
                    "branch_result ok = b{}, error = b{}",
                    v.ok.0, v.error.0
                )
            }
            Instruction::JumpTable(ref v) => {
                format!(
                    "jump_table r{}, {}",
                    v.register.0,
                    v.blocks
                        .iter()
                        .enumerate()
                        .map(|(idx, block)| format!("{} = b{}", idx, block.0))
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            }
            Instruction::False(ref v) => {
                format!("r{} = false", v.register.0)
            }
            Instruction::True(ref v) => {
                format!("r{} = true", v.register.0)
            }
            Instruction::Nil(ref v) => {
                format!("r{} = nil", v.register.0)
            }
            Instruction::Int(ref v) => {
                format!("r{} = int {:?}", v.register.0, v.value)
            }
            Instruction::Float(ref v) => {
                format!("r{} = float {:?}", v.register.0, v.value)
            }
            Instruction::String(ref v) => {
                format!("r{} = string {:?}", v.register.0, v.value)
            }
            Instruction::Goto(ref v) => {
                format!("goto b{}", v.block.0)
            }
            Instruction::MoveRegister(ref v) => {
                format!("r{} = move r{}", v.target.0, v.source.0)
            }
            Instruction::Drop(ref v) => {
                format!("drop r{}", v.register.0)
            }
            Instruction::Free(ref v) => {
                format!("free r{}", v.register.0)
            }
            Instruction::Clone(ref v) => {
                format!(
                    "r{} = clone {:?}(r{})",
                    v.register.0, v.kind, v.source.0
                )
            }
            Instruction::CheckRefs(ref v) => {
                format!("check_refs r{}", v.register.0)
            }
            Instruction::RefKind(ref v) => {
                format!("r{} = ref_kind r{}", v.register.0, v.value.0)
            }
            Instruction::MoveResult(ref v) => {
                format!("r{} = move_result", v.register.0)
            }
            Instruction::Return(ref v) => {
                format!("return r{}", v.register.0)
            }
            Instruction::Throw(ref v) => {
                format!("throw r{}", v.register.0)
            }
            Instruction::ReturnAsync(ref v) => {
                format!("r{} = async return r{}", v.register.0, v.value.0)
            }
            Instruction::ThrowAsync(ref v) => {
                format!("r{} = async throw r{}", v.register.0, v.value.0)
            }
            Instruction::Allocate(ref v) => {
                format!("r{} = allocate {}", v.register.0, v.class.name(db))
            }
            Instruction::AllocateArray(ref v) => {
                format!("r{} = array [{}]", v.register.0, join(&v.values))
            }
            Instruction::Strings(ref v) => {
                format!("r{} = strings [{}]", v.register.0, join(&v.values))
            }
            Instruction::CallStatic(ref v) => {
                format!(
                    "call_static {}.{}({})",
                    v.class.name(db),
                    v.method.name(db),
                    join(&v.arguments)
                )
            }
            Instruction::CallVirtual(ref v) => {
                format!(
                    "call_virtual r{}.{}({})",
                    v.receiver.0,
                    v.method.name(db),
                    join(&v.arguments)
                )
            }
            Instruction::CallDynamic(ref v) => {
                format!(
                    "call_dynamic r{}.{}({})",
                    v.receiver.0,
                    v.method.name(db),
                    join(&v.arguments)
                )
            }
            Instruction::CallClosure(ref v) => {
                format!(
                    "call_closure r{}({})",
                    v.receiver.0,
                    join(&v.arguments)
                )
            }
            Instruction::CallDropper(ref v) => {
                format!("call_dropper r{}", v.receiver.0,)
            }
            Instruction::CallBuiltin(ref v) => {
                format!("call_builtin {}({})", v.id.name(), join(&v.arguments))
            }
            Instruction::RawInstruction(ref v) => {
                format!("raw {}({})", v.opcode.name(), join(&v.arguments))
            }
            Instruction::Send(ref v) => {
                format!(
                    "{} r{}.{}({})",
                    if v.wait { "send" } else { "send_forget" },
                    v.receiver.0,
                    v.method.name(db),
                    join(&v.arguments)
                )
            }
            Instruction::SendAsync(ref v) => {
                format!(
                    "r{} = send_async r{}.{}({})",
                    v.register.0,
                    v.receiver.0,
                    v.method.name(db),
                    join(&v.arguments)
                )
            }
            Instruction::GetField(ref v) => {
                format!(
                    "r{} = get_field r{}.{}",
                    v.register.0,
                    v.receiver.0,
                    v.field.name(db)
                )
            }
            Instruction::SetField(ref v) => {
                format!(
                    "set_field r{}.{} = r{}",
                    v.receiver.0,
                    v.field.name(db),
                    v.value.0
                )
            }
            Instruction::Increment(ref v) => {
                format!("r{} = increment r{}", v.register.0, v.value.0)
            }
            Instruction::Decrement(ref v) => {
                format!("decrement r{}", v.register.0)
            }
            Instruction::DecrementAtomic(ref v) => {
                format!("r{} = decrement_atomic r{}", v.register.0, v.value.0)
            }
            Instruction::GetConstant(ref v) => {
                format!(
                    "r{} = const {}::{}",
                    v.register.0,
                    v.id.module(db).name(db),
                    v.id.name(db)
                )
            }
            Instruction::IntEq(ref v) => {
                format!(
                    "r{} = int_eq r{}, r{}",
                    v.register.0, v.left.0, v.right.0
                )
            }
            Instruction::StringEq(ref v) => {
                format!(
                    "r{} = string_eq r{}, r{}",
                    v.register.0, v.left.0, v.right.0
                )
            }
            Instruction::Reduce(ref v) => format!("reduce {}", v.amount),
            Instruction::Finish(v) => {
                if v.terminate { "terminate" } else { "finish" }.to_string()
            }
        }
    }
}

pub(crate) struct Class {
    pub(crate) id: types::ClassId,
    pub(crate) methods: Vec<types::MethodId>,
}

impl Class {
    pub(crate) fn new(id: types::ClassId) -> Self {
        Self { id, methods: Vec::new() }
    }

    pub(crate) fn add_methods(&mut self, methods: &Vec<Method>) {
        for method in methods {
            self.methods.push(method.id);
        }
    }
}

pub(crate) struct Trait {
    pub(crate) id: types::TraitId,
    pub(crate) methods: Vec<types::MethodId>,
}

impl Trait {
    pub(crate) fn new(id: types::TraitId) -> Self {
        Self { id, methods: Vec::new() }
    }

    pub(crate) fn add_methods(&mut self, methods: &Vec<Method>) {
        for method in methods {
            self.methods.push(method.id);
        }
    }
}

#[derive(Clone)]
pub(crate) struct Module {
    pub(crate) id: types::ModuleId,
    pub(crate) classes: Vec<types::ClassId>,
    pub(crate) constants: Vec<types::ConstantId>,
}

impl Module {
    pub(crate) fn new(id: types::ModuleId) -> Self {
        Self { id, classes: Vec::new(), constants: Vec::new() }
    }
}

#[derive(Clone)]
pub(crate) struct Method {
    pub(crate) id: types::MethodId,
    pub(crate) registers: Registers,
    pub(crate) body: Graph,
}

impl Method {
    pub(crate) fn new(id: types::MethodId) -> Self {
        Self { id, body: Graph::new(), registers: Registers::new() }
    }
}

pub(crate) struct Location {
    /// The module to use for obtaining the file of the location.
    ///
    /// We store the module here instead of the file so we don't need to
    /// duplicate file paths.
    pub(crate) module: types::ModuleId,

    /// The method the location is defined in.
    pub(crate) method: types::MethodId,

    /// The line and column range.
    pub(crate) range: SourceLocation,
}

#[derive(Copy, Clone, Debug)]
pub(crate) struct LocationId(usize);

/// An Inko program in its MIR form.
pub(crate) struct Mir {
    pub(crate) constants: HashMap<types::ConstantId, Constant>,
    pub(crate) modules: IndexMap<types::ModuleId, Module>,
    pub(crate) classes: HashMap<types::ClassId, Class>,
    pub(crate) traits: HashMap<types::TraitId, Trait>,
    pub(crate) methods: HashMap<types::MethodId, Method>,
    pub(crate) closure_classes: HashMap<types::ClosureId, types::ClassId>,
    locations: Vec<Location>,
}

impl Mir {
    pub(crate) fn new() -> Self {
        Self {
            constants: HashMap::new(),
            modules: IndexMap::new(),
            classes: HashMap::new(),
            traits: HashMap::new(),
            methods: HashMap::new(),
            locations: Vec::new(),
            closure_classes: HashMap::new(),
        }
    }

    pub(crate) fn add_methods(&mut self, methods: Vec<Method>) {
        for method in methods {
            self.methods.insert(method.id, method);
        }
    }

    pub(crate) fn add_location(
        &mut self,
        module: types::ModuleId,
        method: types::MethodId,
        range: SourceLocation,
    ) -> LocationId {
        let id = LocationId(self.locations.len());

        self.locations.push(Location { module, method, range });
        id
    }

    pub(crate) fn last_location(&self) -> Option<LocationId> {
        if self.locations.is_empty() {
            None
        } else {
            Some(LocationId(self.locations.len() - 1))
        }
    }

    pub(crate) fn location(&self, index: LocationId) -> &Location {
        &self.locations[index.0]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_constant_eq() {
        assert_eq!(Constant::Float(0.0), Constant::Float(0.0));
        assert_ne!(Constant::Float(0.0), Constant::Float(-0.0));
        assert_ne!(Constant::Float(-0.0), Constant::Float(0.0));
    }
}
