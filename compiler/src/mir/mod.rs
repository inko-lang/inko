//! A mid-level graph IR.
//!
//! MIR is used for various optimisations, analysing moves of values, compiling
//! pattern matching into decision trees, and more.
pub(crate) mod inline;
pub(crate) mod passes;
pub(crate) mod pattern_matching;
pub(crate) mod printer;
pub(crate) mod specialize;

use crate::symbol_names::SymbolNames;
use indexmap::{IndexMap, IndexSet};
use location::Location;
use std::collections::{HashMap, HashSet, VecDeque};
use std::fmt;
use std::hash::{Hash, Hasher};
use std::mem::swap;
use std::ops::{Add, AddAssign, Sub, SubAssign};
use std::sync::Mutex;
use std::thread;
use types::{
    Database, ForeignType, Intrinsic, MethodId, Sign, TypeArguments, TypeEnum,
    TypeRef, BOOL_ID, DROPPER_METHOD, FLOAT_ID, INT_ID, NIL_ID,
};

/// The register ID of the register that stores `self`.
pub(crate) const SELF_ID: usize = 0;

fn join(initial: Option<RegisterId>, values: &[RegisterId]) -> String {
    initial
        .iter()
        .chain(values)
        .map(|v| format!("r{}", v.0))
        .collect::<Vec<_>>()
        .join(", ")
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
        let id = self.values.len() as _;

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

    pub(crate) fn iter_mut(&mut self) -> impl Iterator<Item = &mut Register> {
        self.values.iter_mut()
    }

    pub(crate) fn merge(&mut self, mut other: Registers) {
        // Reserve the exact amount so we don't allocate more memory than
        // necessary, which can have a big impact when e.g. inlining methods.
        self.values.reserve_exact(other.values.len());
        self.values.append(&mut other.values);
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

        // We don't want to allocate more than necessary, and blocks aren't
        // added in tight loops so we explicitly reserve the amount of memory
        // necessary.
        self.blocks.reserve_exact(1);
        self.blocks.push(Block::new());
        id
    }

    pub(crate) fn block_mut(&mut self, index: BlockId) -> &mut Block {
        self.blocks.get_mut(index.0).unwrap()
    }

    pub(crate) fn block(&self, index: BlockId) -> &Block {
        &self.blocks[index.0]
    }

    pub(crate) fn add_edge(&mut self, source: BlockId, target: BlockId) {
        // Edges aren't added that often, and we don't want to allocate more
        // memory than necessary, so we explicitly reserve the exact amount
        // necessary.
        let target_block = &mut self.blocks[target.0];

        target_block.predecessors.reserve_exact(1);
        target_block.predecessors.insert(source);

        let source_block = &mut self.blocks[source.0];

        source_block.successors.reserve_exact(1);
        source_block.successors.insert(target);
    }

    pub(crate) fn is_connected(&self, block: BlockId) -> bool {
        block == self.start_id || !self.blocks[block.0].predecessors.is_empty()
    }

    pub(crate) fn predecessors(&self, block: BlockId) -> IndexSet<BlockId> {
        self.blocks[block.0].predecessors.clone()
    }

    pub(crate) fn successors(&self, block: BlockId) -> IndexSet<BlockId> {
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

    pub(crate) fn merge(&mut self, mut other: Graph) {
        self.blocks.reserve_exact(other.blocks.len());
        self.blocks.append(&mut other.blocks);
    }

    pub(crate) fn each_block_in_order<F: FnMut(BlockId)>(&self, mut func: F) {
        let mut visit = VecDeque::new();
        let mut visited = HashSet::new();
        let start = self.start_id;

        visit.push_back(start);
        visited.insert(start);

        while let Some(id) = visit.pop_front() {
            func(id);

            for &id in &self.block(id).successors {
                if visited.contains(&id) {
                    continue;
                }

                visit.push_back(id);
                visited.insert(id);
            }
        }
    }
}

/// The ID/index to a basic block within a method.
#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq)]
pub(crate) struct BlockId(pub(crate) usize);

impl Add<usize> for BlockId {
    type Output = BlockId;

    fn add(self, rhs: usize) -> Self::Output {
        BlockId(self.0 + rhs)
    }
}

impl AddAssign<usize> for BlockId {
    fn add_assign(&mut self, rhs: usize) {
        self.0 += rhs;
    }
}

impl Sub<usize> for BlockId {
    type Output = BlockId;

    fn sub(self, rhs: usize) -> Self::Output {
        BlockId(self.0 - rhs)
    }
}

impl SubAssign<usize> for BlockId {
    fn sub_assign(&mut self, rhs: usize) {
        self.0 -= rhs;
    }
}

/// A basic block in a control flow graph.
#[derive(Clone)]
pub(crate) struct Block {
    /// The MIR instructions in this block.
    pub(crate) instructions: Vec<Instruction>,

    /// All the successors of this block.
    pub(crate) successors: IndexSet<BlockId>,

    /// All the predecessors of this block.
    pub(crate) predecessors: IndexSet<BlockId>,
}

impl Block {
    pub(crate) fn new() -> Self {
        Self {
            instructions: Vec::new(),
            successors: IndexSet::new(),
            predecessors: IndexSet::new(),
        }
    }

    pub(crate) fn map_edges<F: Fn(BlockId) -> BlockId>(&mut self, func: F) {
        for id in self.take_successors() {
            self.successors.insert(func(id));
        }

        for id in self.take_predecessors() {
            self.predecessors.insert(func(id));
        }
    }

    pub(crate) fn take_successors(&mut self) -> IndexSet<BlockId> {
        let mut vals = IndexSet::new();

        swap(&mut vals, &mut self.successors);
        vals
    }

    pub(crate) fn take_predecessors(&mut self) -> IndexSet<BlockId> {
        let mut vals = IndexSet::new();

        swap(&mut vals, &mut self.predecessors);
        vals
    }

    pub(crate) fn goto(
        &mut self,
        block: BlockId,
        location: InstructionLocation,
    ) {
        self.instructions
            .push(Instruction::Goto(Box::new(Goto { block, location })));
    }

    pub(crate) fn branch(
        &mut self,
        condition: RegisterId,
        if_true: BlockId,
        if_false: BlockId,
        location: InstructionLocation,
    ) {
        self.instructions.push(Instruction::Branch(Box::new(Branch {
            condition,
            if_true,
            if_false,
            location,
        })));
    }

    pub(crate) fn switch(
        &mut self,
        register: RegisterId,
        blocks: Vec<(i64, BlockId)>,
        location: InstructionLocation,
    ) {
        self.instructions.push(Instruction::Switch(Box::new(Switch {
            register,
            blocks,
            fallback: None,
            location,
        })));
    }

    pub(crate) fn switch_with_fallback(
        &mut self,
        register: RegisterId,
        blocks: Vec<(i64, BlockId)>,
        fallback: BlockId,
        location: InstructionLocation,
    ) {
        self.instructions.push(Instruction::Switch(Box::new(Switch {
            register,
            blocks,
            fallback: Some(fallback),
            location,
        })));
    }

    pub(crate) fn return_value(
        &mut self,
        register: RegisterId,
        location: InstructionLocation,
    ) {
        self.instructions
            .push(Instruction::Return(Box::new(Return { register, location })));
    }

    pub(crate) fn finish(
        &mut self,
        terminate: bool,
        location: InstructionLocation,
    ) {
        self.instructions.push(Instruction::Finish(Box::new(Finish {
            location,
            terminate,
        })));
    }

    pub(crate) fn nil_literal(
        &mut self,
        register: RegisterId,
        location: InstructionLocation,
    ) {
        self.instructions.push(Instruction::Nil(Box::new(NilLiteral {
            register,
            location,
        })));
    }

    pub(crate) fn bool_literal(
        &mut self,
        register: RegisterId,
        value: bool,
        location: InstructionLocation,
    ) {
        self.instructions.push(Instruction::Bool(Box::new(BoolLiteral {
            register,
            value,
            location,
        })));
    }

    pub(crate) fn int_literal(
        &mut self,
        register: RegisterId,
        bits: u8,
        value: i64,
        location: InstructionLocation,
    ) {
        self.instructions.push(Instruction::Int(Box::new(IntLiteral {
            register,
            bits,
            value,
            location,
        })));
    }

    pub(crate) fn u16_literal(
        &mut self,
        register: RegisterId,
        value: u16,
        location: InstructionLocation,
    ) {
        self.int_literal(register, 16, value as i64, location)
    }

    pub(crate) fn i64_literal(
        &mut self,
        register: RegisterId,
        value: i64,
        location: InstructionLocation,
    ) {
        self.int_literal(register, 64, value, location)
    }

    pub(crate) fn float_literal(
        &mut self,
        register: RegisterId,
        value: f64,
        location: InstructionLocation,
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
        location: InstructionLocation,
    ) {
        self.instructions.push(Instruction::String(Box::new(StringLiteral {
            register,
            value,
            location,
        })));
    }

    pub(crate) fn move_register(
        &mut self,
        target: RegisterId,
        source: RegisterId,
        location: InstructionLocation,
    ) {
        self.instructions.push(Instruction::MoveRegister(Box::new(
            MoveRegister { source, target, volatile: false, location },
        )));
    }

    pub(crate) fn move_volatile_register(
        &mut self,
        target: RegisterId,
        source: RegisterId,
        location: InstructionLocation,
    ) {
        self.instructions.push(Instruction::MoveRegister(Box::new(
            MoveRegister { source, target, volatile: true, location },
        )));
    }

    pub(crate) fn borrow(
        &mut self,
        register: RegisterId,
        value: RegisterId,
        location: InstructionLocation,
    ) {
        self.instructions.push(Instruction::Borrow(Box::new(Borrow {
            register,
            value,
            location,
        })));
    }

    pub(crate) fn increment(
        &mut self,
        value: RegisterId,
        location: InstructionLocation,
    ) {
        self.instructions.push(Instruction::Increment(Box::new(Increment {
            register: value,
            location,
        })));
    }

    pub(crate) fn decrement(
        &mut self,
        register: RegisterId,
        location: InstructionLocation,
    ) {
        self.instructions.push(Instruction::Decrement(Box::new(Decrement {
            register,
            location,
        })));
    }

    pub(crate) fn increment_atomic(
        &mut self,
        value: RegisterId,
        location: InstructionLocation,
    ) {
        self.instructions.push(Instruction::IncrementAtomic(Box::new(
            IncrementAtomic { register: value, location },
        )));
    }

    pub(crate) fn decrement_atomic(
        &mut self,
        register: RegisterId,
        if_true: BlockId,
        if_false: BlockId,
        location: InstructionLocation,
    ) {
        self.instructions.push(Instruction::DecrementAtomic(Box::new(
            DecrementAtomic { register, if_true, if_false, location },
        )));
    }

    pub(crate) fn drop(
        &mut self,
        register: RegisterId,
        location: InstructionLocation,
    ) {
        self.instructions.push(Instruction::Drop(Box::new(Drop {
            register,
            dropper: true,
            location,
        })));
    }

    pub(crate) fn drop_without_dropper(
        &mut self,
        register: RegisterId,
        location: InstructionLocation,
    ) {
        self.instructions.push(Instruction::Drop(Box::new(Drop {
            register,
            dropper: false,
            location,
        })));
    }

    pub(crate) fn free(
        &mut self,
        register: RegisterId,
        type_id: types::TypeId,
        location: InstructionLocation,
    ) {
        self.instructions.push(Instruction::Free(Box::new(Free {
            register,
            type_id,
            location,
        })));
    }

    pub(crate) fn check_refs(
        &mut self,
        register: RegisterId,
        location: InstructionLocation,
    ) {
        self.instructions.push(Instruction::CheckRefs(Box::new(CheckRefs {
            register,
            location,
        })));
    }

    pub(crate) fn call_static(
        &mut self,
        register: RegisterId,
        method: types::MethodId,
        arguments: Vec<RegisterId>,
        type_arguments: Option<usize>,
        location: InstructionLocation,
    ) {
        self.instructions.push(Instruction::CallStatic(Box::new(CallStatic {
            register,
            method,
            arguments,
            type_arguments,
            location,
        })));
    }

    pub(crate) fn call_instance(
        &mut self,
        register: RegisterId,
        receiver: RegisterId,
        method: types::MethodId,
        arguments: Vec<RegisterId>,
        type_arguments: Option<usize>,
        location: InstructionLocation,
    ) {
        self.instructions.push(Instruction::CallInstance(Box::new(
            CallInstance {
                register,
                receiver,
                method,
                arguments,
                type_arguments,
                location,
            },
        )));
    }

    pub(crate) fn call_extern(
        &mut self,
        register: RegisterId,
        method: types::MethodId,
        arguments: Vec<RegisterId>,
        location: InstructionLocation,
    ) {
        self.instructions.push(Instruction::CallExtern(Box::new(CallExtern {
            register,
            method,
            arguments,
            location,
        })));
    }

    pub(crate) fn call_dynamic(
        &mut self,
        register: RegisterId,
        receiver: RegisterId,
        method: types::MethodId,
        arguments: Vec<RegisterId>,
        type_arguments: Option<usize>,
        location: InstructionLocation,
    ) {
        self.instructions.push(Instruction::CallDynamic(Box::new(
            CallDynamic {
                register,
                receiver,
                method,
                arguments,
                type_arguments,
                location,
            },
        )));
    }

    pub(crate) fn call_closure(
        &mut self,
        register: RegisterId,
        receiver: RegisterId,
        arguments: Vec<RegisterId>,
        location: InstructionLocation,
    ) {
        self.instructions.push(Instruction::CallClosure(Box::new(
            CallClosure { register, receiver, arguments, location },
        )));
    }

    pub(crate) fn call_dropper(
        &mut self,
        register: RegisterId,
        receiver: RegisterId,
        location: InstructionLocation,
    ) {
        self.instructions.push(Instruction::CallDropper(Box::new(
            CallDropper { register, receiver, location },
        )));
    }

    pub(crate) fn call_builtin(
        &mut self,
        register: RegisterId,
        name: Intrinsic,
        arguments: Vec<RegisterId>,
        location: InstructionLocation,
    ) {
        self.instructions.push(Instruction::CallBuiltin(Box::new(
            CallBuiltin { register, name, arguments, location },
        )));
    }

    pub(crate) fn send(
        &mut self,
        receiver: RegisterId,
        method: types::MethodId,
        arguments: Vec<RegisterId>,
        type_arguments: Option<usize>,
        location: InstructionLocation,
    ) {
        self.instructions.push(Instruction::Send(Box::new(Send {
            receiver,
            method,
            arguments,
            type_arguments,
            location,
        })));
    }

    pub(crate) fn get_field(
        &mut self,
        register: RegisterId,
        receiver: RegisterId,
        type_id: types::TypeId,
        field: types::FieldId,
        location: InstructionLocation,
    ) {
        self.instructions.push(Instruction::GetField(Box::new(GetField {
            type_id,
            register,
            receiver,
            field,
            location,
        })));
    }

    pub(crate) fn set_field(
        &mut self,
        receiver: RegisterId,
        type_id: types::TypeId,
        field: types::FieldId,
        value: RegisterId,
        location: InstructionLocation,
    ) {
        self.instructions.push(Instruction::SetField(Box::new(SetField {
            receiver,
            value,
            type_id,
            field,
            location,
        })));
    }

    pub(crate) fn pointer(
        &mut self,
        register: RegisterId,
        value: RegisterId,
        location: InstructionLocation,
    ) {
        self.instructions.push(Instruction::Pointer(Box::new(Pointer {
            register,
            value,
            location,
        })))
    }

    pub(crate) fn field_pointer(
        &mut self,
        register: RegisterId,
        receiver: RegisterId,
        type_id: types::TypeId,
        field: types::FieldId,
        location: InstructionLocation,
    ) {
        self.instructions.push(Instruction::FieldPointer(Box::new(
            FieldPointer { type_id, register, receiver, field, location },
        )));
    }

    pub(crate) fn method_pointer(
        &mut self,
        register: RegisterId,
        method: types::MethodId,
        location: InstructionLocation,
    ) {
        self.instructions.push(Instruction::MethodPointer(Box::new(
            MethodPointer { register, method, location },
        )))
    }

    pub(crate) fn read_pointer(
        &mut self,
        register: RegisterId,
        pointer: RegisterId,
        location: InstructionLocation,
    ) {
        self.instructions.push(Instruction::ReadPointer(Box::new(
            ReadPointer { register, pointer, location },
        )));
    }

    pub(crate) fn write_pointer(
        &mut self,
        pointer: RegisterId,
        value: RegisterId,
        location: InstructionLocation,
    ) {
        self.instructions.push(Instruction::WritePointer(Box::new(
            WritePointer { pointer, value, location },
        )));
    }

    pub(crate) fn allocate(
        &mut self,
        register: RegisterId,
        type_id: types::TypeId,
        location: InstructionLocation,
    ) {
        self.instructions.push(Instruction::Allocate(Box::new(Allocate {
            register,
            type_id,
            location,
        })));
    }

    pub(crate) fn spawn(
        &mut self,
        register: RegisterId,
        type_id: types::TypeId,
        location: InstructionLocation,
    ) {
        self.instructions.push(Instruction::Spawn(Box::new(Spawn {
            register,
            type_id,
            location,
        })));
    }

    pub(crate) fn get_constant(
        &mut self,
        register: RegisterId,
        id: types::ConstantId,
        location: InstructionLocation,
    ) {
        self.instructions.push(Instruction::GetConstant(Box::new(
            GetConstant { register, id, location },
        )));
    }

    pub(crate) fn preempt(&mut self, location: InstructionLocation) {
        self.instructions
            .push(Instruction::Preempt(Box::new(Preempt { location })))
    }

    pub(crate) fn cast(
        &mut self,
        register: RegisterId,
        source: RegisterId,
        from: CastType,
        to: CastType,
        location: InstructionLocation,
    ) {
        self.instructions.push(Instruction::Cast(Box::new(Cast {
            register,
            source,
            from,
            to,
            location,
        })));
    }

    pub(crate) fn size_of(
        &mut self,
        register: RegisterId,
        argument: TypeRef,
        location: InstructionLocation,
    ) {
        self.instructions.push(Instruction::SizeOf(Box::new(SizeOf {
            register,
            argument,
            location,
        })));
    }

    fn split_when<R, W: Fn(&Instruction) -> bool, T: Fn(Instruction) -> R>(
        &mut self,
        when: W,
        then: T,
    ) -> Option<(R, Vec<Instruction>)> {
        if let Some(idx) = self.instructions.iter().position(when) {
            let ins = then(self.instructions.remove(idx));
            let rest = self.instructions.split_off(idx);

            // This ensures we don't keep redundant memory around if
            // the number of instructions was very large.
            self.instructions.shrink_to_fit();
            Some((ins, rest))
        } else {
            None
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) enum Constant {
    Int(i64),
    Float(f64),
    String(String),
    /// For Array constants we also store the type here such that we have access
    /// to the type of every nested Array, instead of only having access to the
    /// type of the outer constant as a whole.
    Array(Vec<Constant>, TypeRef),
    Bool(bool),
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
            (Constant::Array(a, _), Constant::Array(b, _)) => a == b,
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
            Constant::Array(v, _) => v.hash(state),
            Constant::Bool(v) => v.hash(state),
        }
    }
}

impl fmt::Display for Constant {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Int(v) => write!(f, "{}", v),
            Self::Float(v) => write!(f, "{}", v),
            Self::String(v) => write!(f, "{:?}", v),
            Self::Array(v, _) => write!(f, "{:?}", v),
            Self::Bool(v) => write!(f, "{}", v),
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

impl Add<usize> for RegisterId {
    type Output = RegisterId;

    fn add(self, rhs: usize) -> Self::Output {
        RegisterId(self.0 + rhs)
    }
}

impl AddAssign<usize> for RegisterId {
    fn add_assign(&mut self, rhs: usize) {
        self.0 += rhs;
    }
}

/// The location of an instruction.
#[derive(Copy, Clone)]
pub(crate) struct InstructionLocation {
    pub(crate) line: u32,
    pub(crate) column: u32,

    /// The index/ID of the inlined call chain the instruction belongs to.
    ///
    /// The value `u32::MAX` is used to signal a lack of a value.
    pub(crate) inlined_call_id: u32,
}

impl InstructionLocation {
    pub(crate) fn new(location: Location) -> InstructionLocation {
        InstructionLocation {
            line: location.line_start,
            column: location.column_start,
            inlined_call_id: u32::MAX,
        }
    }

    pub(crate) fn set_inlined_call_id(&mut self, offset: u32) {
        if self.inlined_call_id == u32::MAX {
            self.inlined_call_id = offset;
        } else {
            // While triggering an overflow requires _a lot_ of values, it's
            // better to panic in such a case instead of silently wrapping
            // around.
            self.inlined_call_id =
                self.inlined_call_id.checked_add(offset + 1).unwrap();
        }
    }

    pub(crate) fn inlined_call_id(self) -> Option<usize> {
        (self.inlined_call_id != u32::MAX)
            .then_some(self.inlined_call_id as usize)
    }
}

#[derive(Clone)]
pub(crate) struct Branch {
    pub(crate) condition: RegisterId,
    pub(crate) if_true: BlockId,
    pub(crate) if_false: BlockId,
    pub(crate) location: InstructionLocation,
}

#[derive(Clone)]
pub(crate) struct Switch {
    pub(crate) register: RegisterId,
    pub(crate) blocks: Vec<(i64, BlockId)>,
    pub(crate) fallback: Option<BlockId>,
    pub(crate) location: InstructionLocation,
}

impl Switch {
    fn blocks(&self) -> impl Iterator<Item = &BlockId> {
        self.blocks.iter().map(|(_, b)| b).chain(self.fallback.iter())
    }

    fn blocks_mut(&mut self) -> impl Iterator<Item = &mut BlockId> {
        self.blocks.iter_mut().map(|(_, b)| b).chain(self.fallback.iter_mut())
    }
}

#[derive(Clone)]
pub(crate) struct Goto {
    pub(crate) block: BlockId,
    pub(crate) location: InstructionLocation,
}

#[derive(Clone)]
pub(crate) struct MoveRegister {
    pub(crate) target: RegisterId,
    pub(crate) source: RegisterId,
    /// When set to `true`, the instruction must never be optimized away at the
    /// MIR level.
    ///
    /// This flag is/should be set when assigning the registers used for local
    /// variables, otherwise we may optimize them away such that e.g. loops no
    /// longer work.
    pub(crate) volatile: bool,
    pub(crate) location: InstructionLocation,
}

#[derive(Clone)]
pub(crate) struct CheckRefs {
    pub(crate) register: RegisterId,
    pub(crate) location: InstructionLocation,
}

/// Drops a value according to its type.
///
/// If `dropper` is set to `false`, the dropper method isn't called for a value
/// no longer in use.
#[derive(Clone)]
pub(crate) struct Drop {
    pub(crate) register: RegisterId,
    pub(crate) dropper: bool,
    pub(crate) location: InstructionLocation,
}

#[derive(Clone)]
pub(crate) struct CallDropper {
    pub(crate) register: RegisterId,
    pub(crate) receiver: RegisterId,
    pub(crate) location: InstructionLocation,
}

#[derive(Clone)]
pub(crate) struct Free {
    pub(crate) type_id: types::TypeId,
    pub(crate) register: RegisterId,
    pub(crate) location: InstructionLocation,
}

#[derive(Clone)]
pub(crate) struct Borrow {
    pub(crate) register: RegisterId,
    pub(crate) value: RegisterId,
    pub(crate) location: InstructionLocation,
}

#[derive(Clone)]
pub(crate) struct Increment {
    pub(crate) register: RegisterId,
    pub(crate) location: InstructionLocation,
}

#[derive(Clone)]
pub(crate) struct Decrement {
    pub(crate) register: RegisterId,
    pub(crate) location: InstructionLocation,
}

#[derive(Clone)]
pub(crate) struct IncrementAtomic {
    pub(crate) register: RegisterId,
    pub(crate) location: InstructionLocation,
}

#[derive(Clone)]
pub(crate) struct DecrementAtomic {
    pub(crate) register: RegisterId,
    pub(crate) if_true: BlockId,
    pub(crate) if_false: BlockId,
    pub(crate) location: InstructionLocation,
}

#[derive(Clone)]
pub(crate) struct BoolLiteral {
    pub(crate) value: bool,
    pub(crate) register: RegisterId,
    pub(crate) location: InstructionLocation,
}

#[derive(Clone)]
pub(crate) struct NilLiteral {
    pub(crate) register: RegisterId,
    pub(crate) location: InstructionLocation,
}

#[derive(Clone)]
pub(crate) struct Return {
    pub(crate) register: RegisterId,
    pub(crate) location: InstructionLocation,
}

#[derive(Clone)]
pub(crate) struct IntLiteral {
    pub(crate) register: RegisterId,
    pub(crate) bits: u8,
    pub(crate) value: i64,
    pub(crate) location: InstructionLocation,
}

#[derive(Clone)]
pub(crate) struct FloatLiteral {
    pub(crate) register: RegisterId,
    pub(crate) value: f64,
    pub(crate) location: InstructionLocation,
}

#[derive(Clone)]
pub(crate) struct StringLiteral {
    pub(crate) register: RegisterId,
    pub(crate) value: String,
    pub(crate) location: InstructionLocation,
}

#[derive(Clone)]
pub(crate) struct CallStatic {
    pub(crate) register: RegisterId,
    pub(crate) method: types::MethodId,
    pub(crate) arguments: Vec<RegisterId>,
    pub(crate) type_arguments: Option<usize>,
    pub(crate) location: InstructionLocation,
}

#[derive(Clone)]
pub(crate) struct CallInstance {
    pub(crate) register: RegisterId,
    pub(crate) receiver: RegisterId,
    pub(crate) method: types::MethodId,
    pub(crate) arguments: Vec<RegisterId>,
    pub(crate) type_arguments: Option<usize>,
    pub(crate) location: InstructionLocation,
}

#[derive(Clone)]
pub(crate) struct CallExtern {
    pub(crate) register: RegisterId,
    pub(crate) method: types::MethodId,
    pub(crate) arguments: Vec<RegisterId>,
    pub(crate) location: InstructionLocation,
}

#[derive(Clone)]
pub(crate) struct CallDynamic {
    pub(crate) register: RegisterId,
    pub(crate) receiver: RegisterId,
    pub(crate) method: types::MethodId,
    pub(crate) arguments: Vec<RegisterId>,
    pub(crate) type_arguments: Option<usize>,
    pub(crate) location: InstructionLocation,
}

#[derive(Clone)]
pub(crate) struct CallClosure {
    pub(crate) register: RegisterId,
    pub(crate) receiver: RegisterId,
    pub(crate) arguments: Vec<RegisterId>,
    pub(crate) location: InstructionLocation,
}

#[derive(Clone)]
pub(crate) struct CallBuiltin {
    pub(crate) register: RegisterId,
    pub(crate) name: Intrinsic,
    pub(crate) arguments: Vec<RegisterId>,
    pub(crate) location: InstructionLocation,
}

#[derive(Clone)]
pub(crate) struct Send {
    pub(crate) receiver: RegisterId,
    pub(crate) method: types::MethodId,
    pub(crate) arguments: Vec<RegisterId>,
    pub(crate) type_arguments: Option<usize>,
    pub(crate) location: InstructionLocation,
}

#[derive(Clone)]
pub(crate) struct GetField {
    pub(crate) type_id: types::TypeId,
    pub(crate) register: RegisterId,
    pub(crate) receiver: RegisterId,
    pub(crate) field: types::FieldId,
    pub(crate) location: InstructionLocation,
}

#[derive(Clone)]
pub(crate) struct SetField {
    pub(crate) type_id: types::TypeId,
    pub(crate) receiver: RegisterId,
    pub(crate) value: RegisterId,
    pub(crate) field: types::FieldId,
    pub(crate) location: InstructionLocation,
}

#[derive(Clone)]
pub(crate) struct GetConstant {
    pub(crate) register: RegisterId,
    pub(crate) id: types::ConstantId,
    pub(crate) location: InstructionLocation,
}

#[derive(Clone)]
pub(crate) struct Allocate {
    pub(crate) register: RegisterId,
    pub(crate) type_id: types::TypeId,
    pub(crate) location: InstructionLocation,
}

#[derive(Clone)]
pub(crate) struct Spawn {
    pub(crate) register: RegisterId,
    pub(crate) type_id: types::TypeId,
    pub(crate) location: InstructionLocation,
}

#[derive(Clone)]
pub(crate) struct Preempt {
    pub(crate) location: InstructionLocation,
}

#[derive(Clone)]
pub(crate) struct Finish {
    pub(crate) terminate: bool,
    pub(crate) location: InstructionLocation,
}

#[derive(Clone)]
pub(crate) struct Cast {
    pub(crate) register: RegisterId,
    pub(crate) source: RegisterId,
    pub(crate) from: CastType,
    pub(crate) to: CastType,
    pub(crate) location: InstructionLocation,
}

#[derive(Clone, Debug, Copy)]
pub(crate) enum CastType {
    Int(u32, Sign),
    Float(u32),
    Pointer,
    Object,
    Trait,
}

impl CastType {
    fn from(db: &Database, typ: TypeRef) -> CastType {
        if let TypeRef::Pointer(_) = typ {
            CastType::Pointer
        } else {
            match typ.as_type_enum(db) {
                Ok(TypeEnum::Foreign(ForeignType::Int(8, sign))) => {
                    CastType::Int(8, sign)
                }
                Ok(TypeEnum::Foreign(ForeignType::Int(16, sign))) => {
                    CastType::Int(16, sign)
                }
                Ok(TypeEnum::Foreign(ForeignType::Int(32, sign))) => {
                    CastType::Int(32, sign)
                }
                Ok(TypeEnum::Foreign(ForeignType::Int(64, sign))) => {
                    CastType::Int(64, sign)
                }
                Ok(TypeEnum::Foreign(ForeignType::Float(32))) => {
                    CastType::Float(32)
                }
                Ok(TypeEnum::Foreign(ForeignType::Float(64))) => {
                    CastType::Float(64)
                }
                Ok(TypeEnum::TypeInstance(ins)) => match ins.instance_of().0 {
                    BOOL_ID | NIL_ID => CastType::Int(1, Sign::Unsigned),
                    INT_ID => CastType::Int(64, Sign::Signed),
                    FLOAT_ID => CastType::Float(64),
                    _ => CastType::Object,
                },
                Ok(TypeEnum::TraitInstance(_)) => CastType::Trait,
                _ => CastType::Object,
            }
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct Pointer {
    pub(crate) register: RegisterId,
    pub(crate) value: RegisterId,
    pub(crate) location: InstructionLocation,
}

#[derive(Clone, Copy)]
pub(crate) struct MethodPointer {
    pub(crate) register: RegisterId,
    pub(crate) method: types::MethodId,
    pub(crate) location: InstructionLocation,
}

#[derive(Clone)]
pub(crate) struct FieldPointer {
    pub(crate) type_id: types::TypeId,
    pub(crate) register: RegisterId,
    pub(crate) receiver: RegisterId,
    pub(crate) field: types::FieldId,
    pub(crate) location: InstructionLocation,
}

#[derive(Clone, Copy)]
pub(crate) struct ReadPointer {
    pub(crate) register: RegisterId,
    pub(crate) pointer: RegisterId,
    pub(crate) location: InstructionLocation,
}

#[derive(Clone, Copy)]
pub(crate) struct WritePointer {
    pub(crate) pointer: RegisterId,
    pub(crate) value: RegisterId,
    pub(crate) location: InstructionLocation,
}

#[derive(Clone, Copy)]
pub(crate) struct SizeOf {
    pub(crate) register: RegisterId,
    pub(crate) argument: types::TypeRef,
    pub(crate) location: InstructionLocation,
}

/// A MIR instruction.
///
/// When adding a new instruction that acts as an exit for a basic block, make
/// sure to also update the compiler pass that removes empty basic blocks.
#[derive(Clone)]
pub(crate) enum Instruction {
    Branch(Box<Branch>),
    Switch(Box<Switch>),
    Float(Box<FloatLiteral>),
    Goto(Box<Goto>),
    Int(Box<IntLiteral>),
    MoveRegister(Box<MoveRegister>),
    Nil(Box<NilLiteral>),
    Return(Box<Return>),
    String(Box<StringLiteral>),
    Bool(Box<BoolLiteral>),
    CallStatic(Box<CallStatic>),
    CallInstance(Box<CallInstance>),
    CallExtern(Box<CallExtern>),
    CallDynamic(Box<CallDynamic>),
    CallClosure(Box<CallClosure>),
    CallDropper(Box<CallDropper>),
    CallBuiltin(Box<CallBuiltin>),
    Send(Box<Send>),
    GetField(Box<GetField>),
    SetField(Box<SetField>),
    CheckRefs(Box<CheckRefs>),
    Drop(Box<Drop>),
    Free(Box<Free>),
    Borrow(Box<Borrow>),
    Increment(Box<Increment>),
    Decrement(Box<Decrement>),
    IncrementAtomic(Box<IncrementAtomic>),
    DecrementAtomic(Box<DecrementAtomic>),
    Allocate(Box<Allocate>),
    Spawn(Box<Spawn>),
    GetConstant(Box<GetConstant>),
    Preempt(Box<Preempt>),
    Finish(Box<Finish>),
    Cast(Box<Cast>),
    Pointer(Box<Pointer>),
    ReadPointer(Box<ReadPointer>),
    WritePointer(Box<WritePointer>),
    FieldPointer(Box<FieldPointer>),
    MethodPointer(Box<MethodPointer>),
    SizeOf(Box<SizeOf>),
}

impl Instruction {
    pub(crate) fn location(&self) -> InstructionLocation {
        match self {
            Instruction::Branch(ref v) => v.location,
            Instruction::Switch(ref v) => v.location,
            Instruction::Bool(ref v) => v.location,
            Instruction::Goto(ref v) => v.location,
            Instruction::MoveRegister(ref v) => v.location,
            Instruction::Return(ref v) => v.location,
            Instruction::Nil(ref v) => v.location,
            Instruction::Int(ref v) => v.location,
            Instruction::Float(ref v) => v.location,
            Instruction::String(ref v) => v.location,
            Instruction::CallStatic(ref v) => v.location,
            Instruction::CallInstance(ref v) => v.location,
            Instruction::CallExtern(ref v) => v.location,
            Instruction::CallDynamic(ref v) => v.location,
            Instruction::CallClosure(ref v) => v.location,
            Instruction::CallDropper(ref v) => v.location,
            Instruction::CallBuiltin(ref v) => v.location,
            Instruction::Send(ref v) => v.location,
            Instruction::GetField(ref v) => v.location,
            Instruction::SetField(ref v) => v.location,
            Instruction::CheckRefs(ref v) => v.location,
            Instruction::Drop(ref v) => v.location,
            Instruction::Free(ref v) => v.location,
            Instruction::Borrow(ref v) => v.location,
            Instruction::Increment(ref v) => v.location,
            Instruction::Decrement(ref v) => v.location,
            Instruction::IncrementAtomic(ref v) => v.location,
            Instruction::DecrementAtomic(ref v) => v.location,
            Instruction::Allocate(ref v) => v.location,
            Instruction::Spawn(ref v) => v.location,
            Instruction::GetConstant(ref v) => v.location,
            Instruction::Preempt(ref v) => v.location,
            Instruction::Finish(ref v) => v.location,
            Instruction::Cast(ref v) => v.location,
            Instruction::Pointer(ref v) => v.location,
            Instruction::ReadPointer(ref v) => v.location,
            Instruction::WritePointer(ref v) => v.location,
            Instruction::FieldPointer(ref v) => v.location,
            Instruction::MethodPointer(ref v) => v.location,
            Instruction::SizeOf(ref v) => v.location,
        }
    }

    fn format(&self, db: &types::Database, names: &SymbolNames) -> String {
        match self {
            Instruction::Branch(ref v) => {
                format!(
                    "branch r{}, true = b{}, false = b{}",
                    v.condition.0, v.if_true.0, v.if_false.0
                )
            }
            Instruction::Switch(ref v) => {
                let blocks = v
                    .blocks
                    .iter()
                    .map(|(idx, block)| format!("{} = b{}", idx, block.0))
                    .collect::<Vec<_>>()
                    .join(", ");

                if let Some(b) = v.fallback {
                    format!(
                        "switch r{}, {}, fallback = b{}",
                        v.register.0, blocks, b.0
                    )
                } else {
                    format!("switch r{}, {}", v.register.0, blocks)
                }
            }
            Instruction::Bool(ref v) => {
                format!("r{} = {}", v.register.0, v.value)
            }
            Instruction::Nil(ref v) => {
                format!("r{} = nil", v.register.0)
            }
            Instruction::Int(ref v) => {
                format!("r{} = i{} {:?}", v.register.0, v.bits, v.value)
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
                format!(
                    "r{} = move r{}{}",
                    v.target.0,
                    v.source.0,
                    if v.volatile { " (volatile)" } else { "" }
                )
            }
            Instruction::Drop(ref v) => {
                format!("drop r{}", v.register.0)
            }
            Instruction::Free(ref v) => {
                format!("free r{}", v.register.0,)
            }
            Instruction::CheckRefs(ref v) => {
                format!("check_refs r{}", v.register.0)
            }
            Instruction::Return(ref v) => {
                format!("return r{}", v.register.0)
            }
            Instruction::Allocate(ref v) => {
                format!(
                    "r{} = allocate {}",
                    v.register.0, names.types[&v.type_id],
                )
            }
            Instruction::Spawn(ref v) => {
                format!("r{} = spawn {}", v.register.0, names.types[&v.type_id])
            }
            Instruction::CallStatic(ref v) => {
                format!(
                    "r{} = call_static {}({})",
                    v.register.0,
                    names.methods[&v.method],
                    join(None, &v.arguments),
                )
            }
            Instruction::CallInstance(ref v) => {
                format!(
                    "r{} = call_instance {}({})",
                    v.register.0,
                    names.methods[&v.method],
                    join(Some(v.receiver), &v.arguments),
                )
            }
            Instruction::CallExtern(ref v) => {
                format!(
                    "r{} = call_extern {}({})",
                    v.register.0,
                    v.method.name(db),
                    join(None, &v.arguments)
                )
            }
            Instruction::CallDynamic(ref v) => {
                format!(
                    "r{} = call_dynamic {}({})",
                    v.register.0,
                    v.method.name(db),
                    join(Some(v.receiver), &v.arguments),
                )
            }
            Instruction::CallClosure(ref v) => {
                format!(
                    "r{} = call_closure r{}({})",
                    v.register.0,
                    v.receiver.0,
                    join(None, &v.arguments)
                )
            }
            Instruction::CallDropper(ref v) => {
                format!("r{} = call_dropper r{}", v.register.0, v.receiver.0,)
            }
            Instruction::CallBuiltin(ref v) => {
                format!(
                    "r{} = call_builtin {}({})",
                    v.register.0,
                    v.name.name(),
                    join(None, &v.arguments)
                )
            }
            Instruction::Send(ref v) => {
                format!(
                    "send {}({})",
                    names.methods[&v.method],
                    join(Some(v.receiver), &v.arguments),
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
            Instruction::Borrow(ref v) => {
                format!("r{} = borrow r{}", v.register.0, v.value.0)
            }
            Instruction::Increment(ref v) => {
                format!("increment r{}", v.register.0)
            }
            Instruction::Decrement(ref v) => {
                format!("decrement r{}", v.register.0)
            }
            Instruction::IncrementAtomic(ref v) => {
                format!("increment_atomic r{}", v.register.0)
            }
            Instruction::DecrementAtomic(ref v) => {
                format!(
                    "decrement_atomic r{}, true = b{}, false = b{}",
                    v.register.0, v.if_true.0, v.if_false.0
                )
            }
            Instruction::GetConstant(ref v) => {
                format!(
                    "r{} = const {}::{}",
                    v.register.0,
                    v.id.module(db).name(db),
                    v.id.name(db)
                )
            }
            Instruction::Preempt(_) => "preempt".to_string(),
            Instruction::Finish(v) => {
                if v.terminate { "terminate" } else { "finish" }.to_string()
            }
            Instruction::Cast(v) => {
                format!("r{} = r{} as {:?}", v.register.0, v.source.0, v.to)
            }
            Instruction::ReadPointer(v) => {
                format!("r{} = read_pointer r{}", v.register.0, v.pointer.0)
            }
            Instruction::WritePointer(v) => {
                format!("*r{} = r{}", v.pointer.0, v.value.0)
            }
            Instruction::Pointer(v) => {
                format!("r{} = pointer r{}", v.register.0, v.value.0)
            }
            Instruction::FieldPointer(ref v) => {
                format!(
                    "r{} = field_pointer r{}.{}",
                    v.register.0,
                    v.receiver.0,
                    v.field.name(db)
                )
            }
            Instruction::MethodPointer(v) => {
                format!(
                    "r{} = method_pointer {}",
                    v.register.0, names.methods[&v.method],
                )
            }
            Instruction::SizeOf(v) => {
                format!(
                    "r{} = size_of {}",
                    v.register.0,
                    types::format::format_type(db, v.argument)
                )
            }
        }
    }
}

pub(crate) struct Type {
    pub(crate) id: types::TypeId,
    pub(crate) methods: Vec<types::MethodId>,
}

impl Type {
    pub(crate) fn new(id: types::TypeId) -> Self {
        Self { id, methods: Vec::new() }
    }

    pub(crate) fn add_methods(&mut self, methods: &Vec<Method>) {
        for method in methods {
            self.methods.push(method.id);
        }
    }

    pub(crate) fn instance_methods_count(&self, db: &Database) -> usize {
        self.methods.iter().filter(|m| !m.is_static(db)).count()
    }
}

#[derive(Clone)]
pub(crate) struct Module {
    pub(crate) id: types::ModuleId,
    pub(crate) types: Vec<types::TypeId>,
    pub(crate) constants: Vec<types::ConstantId>,
    pub(crate) methods: Vec<types::MethodId>,

    /// The methods inlined into this module.
    ///
    /// This is used to flush incremental compilation caches when necessary.
    pub(crate) inlined_methods: IndexSet<types::MethodId>,
}

impl Module {
    pub(crate) fn new(id: types::ModuleId) -> Self {
        Self {
            id,
            types: Vec::new(),
            constants: Vec::new(),
            methods: Vec::new(),
            inlined_methods: IndexSet::new(),
        }
    }
}

#[derive(Copy, Clone)]
pub(crate) struct InlinedCall {
    /// The ID of the calling method.
    pub(crate) caller: MethodId,

    /// The location of the inlined call site.
    pub(crate) location: InstructionLocation,
}

impl InlinedCall {
    pub(crate) fn new(
        caller: MethodId,
        location: InstructionLocation,
    ) -> InlinedCall {
        InlinedCall { caller, location }
    }
}

#[derive(Clone)]
pub(crate) struct InlinedCalls {
    /// The method the instructions were defined in.
    pub(crate) source_method: MethodId,

    /// The inlined call chain leading up to the source method.
    pub(crate) chain: Vec<InlinedCall>,
}

impl InlinedCalls {
    pub(crate) fn new(
        source_method: MethodId,
        chain: Vec<InlinedCall>,
    ) -> InlinedCalls {
        InlinedCalls { source_method, chain }
    }
}

#[derive(Clone)]
pub(crate) struct Method {
    pub(crate) id: types::MethodId,
    pub(crate) registers: Registers,
    pub(crate) body: Graph,
    pub(crate) arguments: Vec<RegisterId>,
    pub(crate) inlined_calls: Vec<InlinedCalls>,
}

impl Method {
    pub(crate) fn new(id: types::MethodId) -> Self {
        Self {
            id,
            body: Graph::new(),
            registers: Registers::new(),
            arguments: Vec::new(),
            inlined_calls: Vec::new(),
        }
    }

    fn register_use_counts(&self) -> Vec<usize> {
        let mut uses = vec![0_usize; self.registers.len()];

        for block in &self.body.blocks {
            for ins in &block.instructions {
                match ins {
                    Instruction::Branch(i) => {
                        uses[i.condition.0] += 1;
                    }
                    Instruction::Switch(i) => {
                        uses[i.register.0] += 1;
                    }
                    Instruction::MoveRegister(i) => {
                        uses[i.source.0] += 1;

                        if i.volatile {
                            uses[i.target.0] += 1;
                        }
                    }
                    Instruction::Return(i) => {
                        uses[i.register.0] += 1;
                    }
                    Instruction::CallStatic(i) => {
                        i.arguments.iter().for_each(|r| uses[r.0] += 1);
                    }
                    Instruction::CallInstance(i) => {
                        uses[i.receiver.0] += 1;
                        i.arguments.iter().for_each(|r| uses[r.0] += 1);
                    }
                    Instruction::CallExtern(i) => {
                        i.arguments.iter().for_each(|r| uses[r.0] += 1);
                    }
                    Instruction::CallDynamic(i) => {
                        uses[i.receiver.0] += 1;
                        i.arguments.iter().for_each(|r| uses[r.0] += 1);
                    }
                    Instruction::CallClosure(i) => {
                        uses[i.receiver.0] += 1;
                        i.arguments.iter().for_each(|r| uses[r.0] += 1);
                    }
                    Instruction::CallDropper(i) => {
                        uses[i.receiver.0] += 1;
                    }
                    Instruction::CallBuiltin(i) => {
                        i.arguments.iter().for_each(|r| uses[r.0] += 1);
                    }
                    Instruction::Send(i) => {
                        uses[i.receiver.0] += 1;
                        i.arguments.iter().for_each(|r| uses[r.0] += 1);
                    }
                    Instruction::GetField(i) => {
                        uses[i.receiver.0] += 1;
                    }
                    Instruction::SetField(i) => {
                        uses[i.receiver.0] += 1;
                        uses[i.value.0] += 1;
                    }
                    Instruction::CheckRefs(i) => {
                        uses[i.register.0] += 1;
                    }
                    Instruction::Drop(i) => {
                        uses[i.register.0] += 1;
                    }
                    Instruction::Free(i) => {
                        uses[i.register.0] += 1;
                    }
                    Instruction::Borrow(i) => {
                        uses[i.value.0] += 1;
                    }
                    Instruction::Increment(i) => {
                        uses[i.register.0] += 1;
                    }
                    Instruction::Decrement(i) => {
                        uses[i.register.0] += 1;
                    }
                    Instruction::IncrementAtomic(i) => {
                        uses[i.register.0] += 1;
                    }
                    Instruction::DecrementAtomic(i) => {
                        uses[i.register.0] += 1;
                    }
                    Instruction::Cast(i) => {
                        uses[i.source.0] += 1;
                    }
                    Instruction::Pointer(i) => {
                        uses[i.value.0] += 1;
                    }
                    Instruction::ReadPointer(i) => {
                        uses[i.pointer.0] += 1;
                    }
                    Instruction::WritePointer(i) => {
                        uses[i.pointer.0] += 1;
                        uses[i.value.0] += 1;
                    }
                    Instruction::FieldPointer(i) => {
                        uses[i.receiver.0] += 1;
                    }
                    _ => {}
                }
            }
        }

        uses
    }

    fn remove_empty_blocks(&mut self) {
        for idx in 0..self.body.blocks.len() {
            // Unreachable blocks are removed separately, so we can skip them
            // entirely.
            if !self.body.is_connected(BlockId(idx)) {
                continue;
            }

            let (preds, succ) = {
                let block = &mut self.body.blocks[idx];

                if !block.instructions.is_empty() {
                    continue;
                }

                // Empty blocks never have more than one successor. Since we
                // already skip unreachable blocks, we'll also never find a
                // block that doesn't have _any_ successors.
                let succ = block.successors.pop().unwrap();
                let mut pred = IndexSet::new();

                swap(&mut pred, &mut block.predecessors);
                (pred, succ)
            };

            let cur_id = BlockId(idx);

            for pred in preds {
                let block = &mut self.body.blocks[pred.0];

                // If the predecessor block ends with a terminator instruction,
                // we need to make sure the instruction jumps to the _successor_
                // of the current block.
                match block.instructions.last_mut() {
                    Some(Instruction::Goto(ins)) => {
                        ins.block = succ;
                    }
                    Some(Instruction::Branch(ins)) => {
                        if ins.if_true == cur_id {
                            ins.if_true = succ;
                        }

                        if ins.if_false == cur_id {
                            ins.if_false = succ;
                        }
                    }
                    Some(Instruction::Switch(ins)) => {
                        for (_, id) in &mut ins.blocks {
                            if *id == cur_id {
                                *id = succ;
                            }
                        }

                        if let Some(id) = &mut ins.fallback {
                            if *id == cur_id {
                                *id = succ;
                            }
                        }
                    }
                    Some(Instruction::DecrementAtomic(ins)) => {
                        if ins.if_true == cur_id {
                            ins.if_true = succ;
                        }

                        if ins.if_false == cur_id {
                            ins.if_false = succ;
                        }
                    }
                    _ => {}
                }

                block.successors.retain(|i| i.0 != idx);
                self.body.add_edge(pred, succ);
            }

            self.body.blocks[succ.0].predecessors.retain(|i| i.0 != idx);

            if idx == self.body.start_id.0 {
                self.body.start_id = succ;
            }
        }

        // The above loop may make many empty blocks unreachable, so we need to
        // remove such blocks
        self.remove_unreachable_blocks();
    }

    fn remove_unreachable_blocks(&mut self) {
        // This Vec maps block IDs to the value to subtract from the ID in
        // order to derive the ID to use after unreachable blocks are
        // removed.
        let mut shift_map = vec![0; self.body.blocks.len()];
        let mut reachable = vec![false; self.body.blocks.len()];
        let mut queue = Vec::with_capacity(self.body.blocks.len());

        queue.push(self.body.start_id);

        // We don't really care about the order in which we visit blocks, so we
        // just use a Vec here instead of a VecDeque.
        while let Some(id) = queue.pop() {
            reachable[id.0] = true;

            for &edge in &self.body.block(id).successors {
                if !reachable[edge.0] {
                    queue.push(edge);
                }
            }
        }

        let mut shift = reachable.iter().filter(|v| !**v).count();

        for index in (0..self.body.blocks.len()).rev() {
            if !reachable[index] {
                shift -= 1;
            }

            shift_map[index] += shift;
        }

        let num_reachable = reachable.iter().filter(|&&v| v).count();
        let mut blocks = Vec::with_capacity(num_reachable);

        swap(&mut blocks, &mut self.body.blocks);

        for (idx, mut block) in blocks.into_iter().enumerate() {
            if !reachable[idx] {
                continue;
            }

            block.map_edges(|b| b - shift_map[b.0]);

            match block.instructions.last_mut() {
                Some(Instruction::Goto(ins)) => {
                    ins.block -= shift_map[ins.block.0];
                }
                Some(Instruction::Branch(ins)) => {
                    ins.if_true -= shift_map[ins.if_true.0];
                    ins.if_false -= shift_map[ins.if_false.0];
                }
                Some(Instruction::Switch(ins)) => {
                    for (_, id) in &mut ins.blocks {
                        *id -= shift_map[id.0];
                    }

                    if let Some(id) = &mut ins.fallback {
                        *id -= shift_map[id.0];
                    }
                }
                Some(Instruction::DecrementAtomic(ins)) => {
                    ins.if_true -= shift_map[ins.if_true.0];
                    ins.if_false -= shift_map[ins.if_false.0];
                }
                _ => {}
            }

            self.body.blocks.push(block);
        }

        self.body.start_id -= shift_map[self.body.start_id.0];
    }

    /// Applies optimizations local to this method (e.g. they don't depend on
    /// other methods).
    fn apply_local_optimizations(
        &mut self,
        constants: &IndexMap<types::ConstantId, Constant>,
    ) {
        self.merge_switch_blocks();
        self.merge_goto_blocks();
        self.compact_switch();

        // The above code is likely to produce many unreachable basic
        // blocks, so we need to remove those.
        self.remove_unreachable_blocks();
        self.remove_unused_instructions();
        self.inline_constants(constants);
    }

    /// Modifies switch instructions such that branches to blocks that just
    /// contain gotos are turned into branches to the target block.
    ///
    /// These kind of instructions may be generated when compiling `match`
    /// expressions, such as when there are many branches/patterns without any
    /// variables or registers to drop.
    fn merge_switch_blocks(&mut self) {
        let mut map = IndexMap::new();
        let mut links = IndexSet::new();

        for (idx, src) in self.body.blocks.iter().enumerate() {
            let src_id = BlockId(idx);
            let ins = match src.instructions.last() {
                Some(Instruction::Switch(ins)) => ins,
                _ => continue,
            };

            for target_id in ins.blocks() {
                let target = self.body.block(*target_id);

                if target.predecessors.len() > 1
                    || target.instructions.len() > 1
                {
                    continue;
                }

                let new_target = match target.instructions.last() {
                    Some(Instruction::Goto(ins)) => ins.block,
                    _ => continue,
                };

                links.insert((src_id, new_target));
                map.insert((src_id, *target_id), new_target);
            }
        }

        for (idx, src) in self.body.blocks.iter_mut().enumerate() {
            let src_id = BlockId(idx);
            let ins = match src.instructions.last_mut() {
                Some(Instruction::Switch(ins)) => ins,
                _ => continue,
            };

            let mut succ = IndexSet::new();

            for target_id in ins.blocks_mut() {
                if let Some(&new) = map.get(&(src_id, *target_id)) {
                    *target_id = new;
                }

                succ.insert(*target_id);
            }

            src.successors = succ;
        }

        for (_, id) in map.into_keys() {
            let blk = self.body.block_mut(id);

            blk.predecessors.clear();
            blk.successors.clear();
            blk.instructions.clear();
        }

        for (from, to) in links {
            self.body.add_edge(from, to);
        }
    }

    /// Compacts switch instructions by merging branches that all jump to the
    /// same block.
    ///
    /// This change doesn't matter much in terms of performance because LLVM
    /// also applies this optimization, but it makes visualization easier and
    /// reduces the inline weight, making it more likely for the code to be
    /// inlined.
    ///
    /// This optimization isn't applied if the switch has a fallback case, as
    /// for such instructions we don't know what switch/test value to use for
    /// the fallback.
    fn compact_switch(&mut self) {
        let mut map = IndexMap::new();

        for block in &mut self.body.blocks {
            for ins in &mut block.instructions {
                let ins = match ins {
                    Instruction::Switch(v) if v.fallback.is_none() => v,
                    _ => continue,
                };

                for (_, blk) in &ins.blocks {
                    *map.entry(*blk).or_insert(0_usize) += 1;
                }

                if map.len() == 2 {
                    let mut pairs = map.iter();
                    let (&blk1, &len) = pairs.next().unwrap();
                    let (&blk2, _) = pairs.next().unwrap();
                    let (test, fallback) =
                        if len == 1 { (blk1, blk2) } else { (blk2, blk1) };
                    let (val, _) = ins
                        .blocks
                        .iter()
                        .find(|(_, blk)| *blk == test)
                        .unwrap();

                    ins.blocks = vec![(*val, test)];
                    ins.fallback = Some(fallback);
                }

                map.clear();
            }
        }
    }

    /// Merges blocks that end with a "goto" with their successor whenever
    /// possible.
    fn merge_goto_blocks(&mut self) {
        let mut idx = 0;

        while idx < self.body.blocks.len() {
            let block = &self.body.blocks[idx];
            let merge =
                if let Some(Instruction::Goto(_)) = block.instructions.last() {
                    // We need to make sure the target block isn't the start of
                    // a loop, as in that case we can't merge the blocks.
                    block.successors.len() == 1
                        && self.body.blocks[block.successors[0].0]
                            .predecessors
                            .len()
                            == 1
                        && block.successors[0] != self.body.start_id
                } else {
                    false
                };

            if merge {
                let mut next_ins = Vec::new();
                let next_id = block.successors[0];
                let next_block = &mut self.body.blocks[next_id.0];
                let next_succ = next_block.take_successors();

                swap(&mut next_ins, &mut next_block.instructions);
                next_block.predecessors.clear();

                let block = &mut self.body.blocks[idx];

                block.successors.clear();
                block.instructions.pop();
                block.instructions.append(&mut next_ins);

                // Connect the block we merged into with the successors of
                // the block we merged.
                for id in next_succ {
                    self.body.remove_predecessor(id, next_id);
                    self.body.add_edge(BlockId(idx), id);
                }
            } else {
                // Merging a block into its predecessor may result in a new
                // goto() at the end that requires merging, so we only
                // advance if there's nothing to merge.
                idx += 1;
            }
        }
    }

    /// Removes instructions that write to an unused register without side
    /// effects.
    ///
    /// Instructions such as `Int` and `String` don't produce side effects,
    /// meaning that if the register they write to isn't used, the entire
    /// instruction can be removed.
    ///
    /// This method isn't terribly useful on its own, but when combined with
    /// e.g. copy propagation it can result in the removal of many redundant
    /// instructions.
    pub(crate) fn remove_unused_instructions(&mut self) {
        let mut uses = self.register_use_counts();
        let mut repeat = true;

        // Removing an instruction may result in other instructions becoming
        // unused, so we repeat this until we run out of instructions to
        // remove.
        while repeat {
            repeat = false;

            for block in &mut self.body.blocks {
                block.instructions.retain(|ins| {
                    let (reg, src) = match ins {
                        Instruction::Float(i) => (i.register, None),
                        Instruction::Int(i) => (i.register, None),
                        Instruction::Nil(i) => (i.register, None),
                        Instruction::String(i) => (i.register, None),
                        Instruction::Bool(i) => (i.register, None),
                        Instruction::Allocate(i) => (i.register, None),
                        Instruction::Spawn(i) => (i.register, None),
                        Instruction::GetConstant(i) => (i.register, None),
                        Instruction::MethodPointer(i) => (i.register, None),
                        Instruction::SizeOf(i) => (i.register, None),
                        Instruction::MoveRegister(i) => {
                            (i.target, Some(i.source))
                        }
                        Instruction::GetField(i) => {
                            (i.register, Some(i.receiver))
                        }
                        Instruction::FieldPointer(i) => {
                            (i.register, Some(i.receiver))
                        }
                        Instruction::Cast(i) => (i.register, Some(i.source)),
                        _ => return true,
                    };

                    if uses[reg.0] > 0 {
                        return true;
                    }

                    if let Some(src) = src {
                        uses[src.0] -= 1;
                        repeat = true;
                    }

                    false
                });
            }
        }
    }

    /// Inlines trivial constant references.
    ///
    /// Constants such as integers and floats are redundant as we can use their
    /// values directly instead. In fact, doing so avoids unnecessary loads and
    /// can improve performance.
    pub(crate) fn inline_constants(
        &mut self,
        constants: &IndexMap<types::ConstantId, Constant>,
    ) {
        for block in &mut self.body.blocks {
            for ins in &mut block.instructions {
                let op = match ins {
                    Instruction::GetConstant(v) => v,
                    _ => continue,
                };

                let new_ins = match &constants[&op.id] {
                    Constant::Int(v) => {
                        Instruction::Int(Box::new(IntLiteral {
                            register: op.register,
                            bits: 64,
                            value: *v,
                            location: op.location,
                        }))
                    }
                    Constant::Float(v) => {
                        Instruction::Float(Box::new(FloatLiteral {
                            register: op.register,
                            value: *v,
                            location: op.location,
                        }))
                    }
                    Constant::Bool(v) => {
                        Instruction::Bool(Box::new(BoolLiteral {
                            register: op.register,
                            value: *v,
                            location: op.location,
                        }))
                    }
                    _ => continue,
                };

                *ins = new_ins;
            }
        }
    }
}

/// An Inko program in its MIR form.
pub(crate) struct Mir {
    pub(crate) constants: IndexMap<types::ConstantId, Constant>,
    pub(crate) modules: IndexMap<types::ModuleId, Module>,
    pub(crate) types: IndexMap<types::TypeId, Type>,
    pub(crate) methods: IndexMap<types::MethodId, Method>,

    /// Externally defined methods/functions that are called at some point.
    ///
    /// As part of specialization we "reset" the MIR database such that after
    /// specialization, only used and specialized types/methods remain. This set
    /// is used to track which external methods are called, such that we only
    /// process those when generating machine code.
    pub(crate) extern_methods: IndexSet<types::MethodId>,

    /// The type arguments to expose to call instructions, used to specialize
    /// types and method calls.
    ///
    /// This data is stored out of bounds and addressed through an index, as
    /// it's only needed by the specialization pass, and this makes it easy to
    /// remove the data once we no longer need it.
    pub(crate) type_arguments: Vec<TypeArguments>,

    /// Methods called through traits/dynamic dispatch.
    ///
    /// This is used to determine what methods we need to generate dynamic
    /// dispatch hashes for.
    pub(crate) dynamic_calls:
        IndexMap<MethodId, IndexSet<(MethodId, Vec<TypeRef>)>>,
}

impl Mir {
    pub(crate) fn new() -> Self {
        Self {
            constants: IndexMap::new(),
            modules: IndexMap::new(),
            types: IndexMap::new(),
            methods: IndexMap::new(),
            extern_methods: IndexSet::new(),
            type_arguments: Vec::new(),
            dynamic_calls: IndexMap::new(),
        }
    }

    pub(crate) fn add_methods(&mut self, methods: Vec<Method>) {
        for method in methods {
            self.methods.insert(method.id, method);
        }
    }

    pub(crate) fn add_type_arguments(
        &mut self,
        arguments: TypeArguments,
    ) -> Option<usize> {
        if arguments.is_empty() {
            None
        } else {
            self.type_arguments.push(arguments);
            Some(self.type_arguments.len() - 1)
        }
    }

    pub(crate) fn sort(&mut self, db: &Database, names: &SymbolNames) {
        // We sort the data by their generated symbol names, as these are
        // already unique for each ID and take into account data such as the
        // type arguments. If we sorted just by IDs we'd get an inconsistent
        // order between compilations, and if we just sorted by names we may get
        // an inconsistent order when many values share the same name.
        for module in self.modules.values_mut() {
            module.constants.sort_by_key(|i| &names.constants[i]);
            module.types.sort_by_key(|i| &names.types[i]);
            module.methods.sort_by_key(|i| &names.methods[i]);
        }

        for typ in self.types.values_mut() {
            typ.methods.sort_by_key(|i| &names.methods[i]);
        }

        // When populating object caches we need to be able to iterate over the
        // MIR modules in a stable order. We do this here (and once) such that
        // from this point forward, we can rely on a stable order, as it's too
        // easy to forget to first sort this list every time we want to iterate
        // over it.
        //
        // Because `mir.modules` is an IndexMap, sorting it is a bit more
        // involved compared to just sorting a `Vec`.
        let mut modules = IndexMap::new();

        swap(&mut modules, &mut self.modules);

        let mut values: Vec<_> = modules.into_values().collect();

        values.sort_by_key(|m| m.id.name(db));

        for module in values {
            self.modules.insert(module.id, module);
        }

        // Also sort the method objects themselves, so passes that wish to
        // iterate over this data directly can do so in a stable order.
        let mut methods = IndexMap::new();

        swap(&mut methods, &mut self.methods);

        let mut methods: Vec<_> = methods.into_values().collect();

        methods.sort_by_key(|m| &names.methods[&m.id]);

        for method in methods {
            self.methods.insert(method.id, method);
        }
    }

    pub(crate) fn remove_empty_blocks(&mut self) {
        for method in self.methods.values_mut() {
            method.remove_empty_blocks();
        }
    }

    pub(crate) fn terminate_basic_blocks(&mut self) {
        for method in self.methods.values_mut() {
            for block in &mut method.body.blocks {
                let location = match block.instructions.last() {
                    Some(
                        Instruction::Branch(_)
                        | Instruction::Switch(_)
                        | Instruction::Return(_)
                        | Instruction::Goto(_)
                        | Instruction::DecrementAtomic(_),
                    ) => continue,
                    Some(ins) if block.successors.len() == 1 => ins.location(),
                    _ => continue,
                };

                block.instructions.push(Instruction::Goto(Box::new(Goto {
                    block: block.successors[0],
                    location,
                })));
            }
        }
    }

    pub(crate) fn remove_unused_methods(&mut self, db: &Database) {
        let mut used = vec![false; db.number_of_methods()];

        // `Main.main` is always used because it's the entry point.
        used[db.main_method().unwrap().0 as usize] = true;

        for method in self.methods.values() {
            for block in &method.body.blocks {
                for ins in &block.instructions {
                    match ins {
                        Instruction::CallStatic(i) => {
                            used[i.method.0 as usize] = true;
                        }
                        Instruction::CallInstance(i) => {
                            used[i.method.0 as usize] = true;
                        }
                        Instruction::Send(i) => {
                            used[i.method.0 as usize] = true;
                        }
                        // Extern methods with a body shouldn't be removed if we
                        // create pointers to them.
                        Instruction::MethodPointer(i) => {
                            used[i.method.0 as usize] = true;
                        }
                        Instruction::CallDynamic(i) => {
                            let id = i.method;
                            let tid = id
                                .receiver(db)
                                .as_trait_instance(db)
                                .unwrap()
                                .instance_of();

                            // For dynamic dispatch call sites we'll flag all
                            // possible target methods as used, since we can't
                            // statically determine which implementation is
                            // called.
                            for &typ in tid.implemented_by(db) {
                                let method_impl =
                                    typ.method(db, id.name(db)).unwrap();
                                let mut methods =
                                    method_impl.specializations(db);

                                if methods.is_empty() {
                                    methods.push(method_impl);
                                }

                                for id in methods {
                                    used[id.0 as usize] = true;
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        // If all methods are used (unlikely but certainly possible) then
        // there's nothing else to do.
        if used.iter().filter(|&&v| v).count() == self.methods.len() {
            return;
        }

        let mut removed = vec![false; db.number_of_methods()];
        let mut methods = IndexMap::new();

        swap(&mut methods, &mut self.methods);

        for method in methods.into_values() {
            // We don't inline closures at this stage, so any methods defined on
            // closures are kept.
            //
            // Dropper methods are never inlined but called through a dedicated
            // instruction with the exact receiver type not always being known,
            // so these too we must always keep.
            let keep = method
                .id
                .receiver(db)
                .type_id(db)
                .is_some_and(|v| v.is_closure(db))
                || used[method.id.0 as usize]
                || method.id.name(db) == DROPPER_METHOD;

            if keep {
                self.methods.insert(method.id, method);
            } else {
                removed[method.id.0 as usize] = true;
            }
        }

        for module in self.modules.values_mut() {
            module.methods.retain(|i| !removed[i.0 as usize]);
        }

        for typ in self.types.values_mut() {
            typ.methods.retain(|i| !removed[i.0 as usize]);
        }
    }

    /// Applies method-local optimizations to all methods.
    ///
    /// The optimizations are applied in parallel as they don't rely on any
    /// shared (mutable) state.
    pub(crate) fn apply_method_local_optimizations(&mut self, threads: usize) {
        let queue = Mutex::new(self.methods.values_mut().collect::<Vec<_>>());
        let consts = &self.constants;

        thread::scope(|s| {
            for _ in 0..threads {
                s.spawn(|| loop {
                    let Some(m) = queue.lock().unwrap().pop() else { break };

                    m.apply_local_optimizations(consts);
                });
            }
        });
    }

    /// Removes constant definitions that are never referenced.
    ///
    /// Trivial constants (e.g. integers) are inlined, and those simply never
    /// referred to aren't useful to keep around. We remove such constants in
    /// order to reduce the executable size.
    pub(crate) fn remove_unused_constants(&mut self, db: &Database) {
        let mut used = vec![false; db.number_of_constants()];

        for method in self.methods.values() {
            for block in &method.body.blocks {
                for ins in &block.instructions {
                    let id = if let Instruction::GetConstant(op) = ins {
                        op.id
                    } else {
                        continue;
                    };

                    used[id.0] = true;
                }
            }
        }

        for module in self.modules.values_mut() {
            module.constants.retain(|v| used[v.0]);
        }

        self.constants.retain(|k, _| used[k.0]);
    }

    pub(crate) fn verify(
        &self,
        db: &Database,
        names: &SymbolNames,
    ) -> Result<(), String> {
        self.check_for_duplicate_modules(db)?;
        self.check_for_duplicate_methods(names)?;
        Ok(())
    }

    fn check_for_duplicate_modules(&self, db: &Database) -> Result<(), String> {
        let mut set = HashSet::new();
        let mut dupes = Vec::new();

        for m in self.modules.keys() {
            let name = m.name(db);

            if !set.insert(name) {
                dupes.push(name.to_string());
            }
        }

        if dupes.is_empty() {
            Ok(())
        } else {
            dupes.sort();
            Err(format!(
                "found one or more modules with the same name: {}",
                dupes.join(", ")
            ))
        }
    }

    pub(crate) fn check_for_duplicate_methods(
        &self,
        names: &SymbolNames,
    ) -> Result<(), String> {
        let mut map: HashMap<&String, Vec<MethodId>> = HashMap::new();
        let mut dupes = false;

        for &m in self.methods.keys() {
            let name = &names.methods[&m];
            let entry = map.entry(name).or_default();

            if !entry.is_empty() {
                dupes = true;
            }

            entry.push(m);
        }

        if dupes {
            let mut names: Vec<_> = map
                .into_iter()
                .filter_map(|(k, v)| {
                    if v.len() > 1 {
                        Some(format!("{} with IDs {:?}", k, v))
                    } else {
                        None
                    }
                })
                .collect();

            names.sort();
            Err(format!(
                "found one or more methods with the same name: {}",
                names.join(", ")
            ))
        } else {
            Ok(())
        }
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

    #[test]
    fn test_method_remove_unreachable_blocks() {
        let mut method = Method::new(MethodId(0));

        let b0 = method.body.add_block();
        let b1 = method.body.add_block();
        let b2 = method.body.add_block();
        let b3 = method.body.add_block();
        let b4 = method.body.add_block();
        let loc = InstructionLocation::new(Location::default());

        method.body.block_mut(b0).goto(b4, loc);
        method.body.add_edge(b0, b4);

        method.body.start_id = b3;
        method.body.block_mut(b3).goto(b2, loc);
        method.body.add_edge(b3, b2);
        method.body.block_mut(b2).goto(b1, loc);
        method.body.add_edge(b2, b1);
        method.remove_unreachable_blocks();

        assert_eq!(method.body.start_id, BlockId(2));
        assert_eq!(method.body.blocks.len(), 3);
    }

    #[test]
    fn test_method_remove_unreachable_blocks_visit_same_block_multiple_times() {
        let mut method = Method::new(MethodId(0));

        let b0 = method.body.add_block();
        let b1 = method.body.add_block();
        let b2 = method.body.add_block();
        let b3 = method.body.add_block();
        let b4 = method.body.add_block();
        let b5 = method.body.add_block();
        let loc = InstructionLocation::new(Location::default());

        method.body.start_id = b0;

        method.body.block_mut(b0).goto(b4, loc);
        method.body.add_edge(b0, b4);

        method.body.block_mut(b1).branch(RegisterId(0), b2, b3, loc);
        method.body.add_edge(b1, b2);
        method.body.add_edge(b1, b3);
        method.body.block_mut(b2).branch(RegisterId(0), b3, b3, loc);

        // We add duplicate edges here so we can test what happens if we visit
        // the same block multiple times. This in turn is important to test for
        // since we don't want to shift multiple times for the same unreachable
        // block.
        method.body.add_edge(b2, b3);
        method.body.add_edge(b2, b3);
        method.body.add_edge(b2, b3);
        method.body.block_mut(b3).goto(b4, loc);
        method.body.add_edge(b3, b4);
        method.body.add_edge(b4, b5);

        method.remove_unreachable_blocks();

        assert_eq!(method.body.start_id, BlockId(0));
        assert_eq!(method.body.blocks.len(), 3);
    }

    #[test]
    fn test_method_remove_unreachable_blocks_with_cycles() {
        let mut method = Method::new(MethodId(0));

        let b0 = method.body.add_block();
        let b1 = method.body.add_block();
        let b2 = method.body.add_block();
        let loc = InstructionLocation::new(Location::default());

        method.body.start_id = b0;
        method.body.block_mut(b1).goto(b2, loc);
        method.body.add_edge(b1, b2);
        method.body.block_mut(b2).goto(b1, loc);
        method.body.add_edge(b2, b1);

        method.remove_unreachable_blocks();

        assert_eq!(method.body.start_id, BlockId(0));
        assert_eq!(method.body.blocks.len(), 1);
    }

    #[test]
    fn test_method_remove_empty_blocks() {
        let mut method = Method::new(MethodId(0));

        let b0 = method.body.add_block();
        let b1 = method.body.add_block();
        let b2 = method.body.add_block();
        let b3 = method.body.add_block();
        let b4 = method.body.add_block();
        let loc = InstructionLocation::new(Location::default());

        //     b0
        //    /  \
        //   b1  b2
        //   |    |
        //   b3  b4
        method.body.start_id = b0;
        method.body.add_edge(b0, b1);
        method.body.add_edge(b0, b2);
        method.body.block_mut(b0).switch(
            RegisterId(0),
            vec![(0, b1), (1, b2)],
            loc,
        );

        method.body.add_edge(b1, b3);
        method.body.add_edge(b2, b4);
        method.body.block_mut(b3).return_value(RegisterId(10), loc);
        method.body.block_mut(b4).return_value(RegisterId(20), loc);

        method.remove_empty_blocks();

        let Some(Instruction::Switch(ins)) =
            method.body.blocks[0].instructions.last()
        else {
            unreachable!()
        };

        assert_eq!(method.body.blocks.len(), 3);
        assert_eq!(ins.blocks, vec![(0, BlockId(1)), (1, BlockId(2))]);
    }

    #[test]
    fn test_merge_switch_blocks() {
        let mut method = Method::new(MethodId(0));

        let b0 = method.body.add_block();
        let b1 = method.body.add_block();
        let b2 = method.body.add_block();
        let loc = InstructionLocation::new(Location::default());

        method.body.start_id = b0;
        method.body.add_edge(b0, b1);
        method.body.add_edge(b1, b2);
        method.body.block_mut(b0).switch(
            RegisterId(0),
            vec![(0, b1), (1, b2)],
            loc,
        );
        method.body.block_mut(b1).goto(b2, loc);
        method.body.block_mut(b2).return_value(RegisterId(10), loc);

        method.merge_switch_blocks();

        let Some(Instruction::Switch(ins)) =
            method.body.blocks[0].instructions.last()
        else {
            unreachable!()
        };

        assert_eq!(ins.blocks, [(0, b2), (1, b2)]);
        assert!(method.body.block(b1).predecessors.is_empty());
        assert!(method.body.block(b1).successors.is_empty());
    }

    #[test]
    fn test_merge_goto_blocks() {
        let mut method = Method::new(MethodId(0));

        let b0 = method.body.add_block();
        let b1 = method.body.add_block();
        let b2 = method.body.add_block();
        let loc = InstructionLocation::new(Location::default());

        method.body.start_id = b0;
        method.body.add_edge(b0, b1);
        method.body.add_edge(b1, b2);
        method.body.block_mut(b0).goto(b1, loc);
        method.body.block_mut(b1).goto(b2, loc);
        method.body.block_mut(b2).return_value(RegisterId(10), loc);

        method.merge_goto_blocks();

        assert_eq!(method.body.block(b0).instructions.len(), 1);
        assert!(method.body.block(b0).predecessors.is_empty());
        assert!(method.body.block(b0).successors.is_empty());
        assert!(method.body.block(b1).predecessors.is_empty());
        assert!(method.body.block(b1).successors.is_empty());
        assert!(method.body.block(b2).instructions.is_empty());
    }

    #[test]
    fn test_compact_switch() {
        let mut method = Method::new(MethodId(0));

        let b0 = method.body.add_block();
        let b1 = method.body.add_block();
        let b2 = method.body.add_block();
        let loc = InstructionLocation::new(Location::default());

        method.body.start_id = b0;
        method.body.add_edge(b0, b1);
        method.body.add_edge(b0, b2);
        method.body.block_mut(b0).switch(
            RegisterId(0),
            vec![(0, b1), (1, b2), (2, b1)],
            loc,
        );
        method.body.block_mut(b1).return_value(RegisterId(10), loc);
        method.body.block_mut(b2).return_value(RegisterId(20), loc);

        method.compact_switch();

        let Some(Instruction::Switch(ins)) =
            method.body.blocks[0].instructions.last()
        else {
            unreachable!()
        };

        assert_eq!(ins.blocks, [(1, b2)]);
        assert_eq!(ins.fallback, Some(b1));
    }

    #[test]
    fn test_compact_switch_with_fallback() {
        let mut method = Method::new(MethodId(0));

        let b0 = method.body.add_block();
        let b1 = method.body.add_block();
        let b2 = method.body.add_block();
        let loc = InstructionLocation::new(Location::default());

        method.body.start_id = b0;
        method.body.add_edge(b0, b1);
        method.body.add_edge(b0, b2);
        method.body.block_mut(b0).switch_with_fallback(
            RegisterId(0),
            vec![(0, b1), (1, b2), (2, b1)],
            b2,
            loc,
        );
        method.body.block_mut(b1).return_value(RegisterId(10), loc);
        method.body.block_mut(b2).return_value(RegisterId(20), loc);

        method.compact_switch();

        let Some(Instruction::Switch(ins)) =
            method.body.blocks[0].instructions.last()
        else {
            unreachable!()
        };

        assert_eq!(ins.blocks, [(0, b1), (1, b2), (2, b1)]);
        assert_eq!(ins.fallback, Some(b2));
    }
}
