//! A mid-level graph IR.
//!
//! MIR is used for various optimisations, analysing moves of values, compiling
//! pattern matching into decision trees, and more.
use ast::source_location::SourceLocation;
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::hash::{Hash, Hasher};
use std::rc::Rc;
use types::collections::IndexMap;
use types::BuiltinFunction;

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
        let id = self.values.len() as _;

        self.values.push(Register { value_type });
        RegisterId(id)
    }

    pub(crate) fn get(&self, register: RegisterId) -> &Register {
        &self.values[register.0 as usize]
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
        block == self.start_id || !self.blocks[block.0].predecessors.is_empty()
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

    pub(crate) fn switch(
        &mut self,
        register: RegisterId,
        blocks: Vec<BlockId>,
        location: LocationId,
    ) {
        self.instructions.push(Instruction::Switch(Box::new(Switch {
            register,
            blocks,
            location,
        })));
    }

    pub(crate) fn switch_kind(
        &mut self,
        register: RegisterId,
        blocks: Vec<BlockId>,
        location: LocationId,
    ) {
        self.instructions.push(Instruction::SwitchKind(Box::new(SwitchKind {
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

    pub(crate) fn reference(
        &mut self,
        register: RegisterId,
        value: RegisterId,
        location: LocationId,
    ) {
        self.instructions.push(Instruction::Reference(Box::new(Reference {
            register,
            value,
            location,
        })));
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

    pub(crate) fn increment_atomic(
        &mut self,
        register: RegisterId,
        value: RegisterId,
        location: LocationId,
    ) {
        self.instructions.push(Instruction::IncrementAtomic(Box::new(
            IncrementAtomic { register, value, location },
        )));
    }

    pub(crate) fn decrement_atomic(
        &mut self,
        register: RegisterId,
        if_true: BlockId,
        if_false: BlockId,
        location: LocationId,
    ) {
        self.instructions.push(Instruction::DecrementAtomic(Box::new(
            DecrementAtomic { register, if_true, if_false, location },
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

    pub(crate) fn call_static(
        &mut self,
        register: RegisterId,
        class: types::ClassId,
        method: types::MethodId,
        arguments: Vec<RegisterId>,
        location: LocationId,
    ) {
        self.instructions.push(Instruction::CallStatic(Box::new(CallStatic {
            register,
            class,
            method,
            arguments,
            location,
        })));
    }

    pub(crate) fn call_instance(
        &mut self,
        register: RegisterId,
        receiver: RegisterId,
        method: types::MethodId,
        arguments: Vec<RegisterId>,
        location: LocationId,
    ) {
        self.instructions.push(Instruction::CallInstance(Box::new(
            CallInstance { register, receiver, method, arguments, location },
        )));
    }

    pub(crate) fn call_extern(
        &mut self,
        register: RegisterId,
        method: types::MethodId,
        arguments: Vec<RegisterId>,
        location: LocationId,
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
        location: LocationId,
    ) {
        self.instructions.push(Instruction::CallDynamic(Box::new(
            CallDynamic { register, receiver, method, arguments, location },
        )));
    }

    pub(crate) fn call_closure(
        &mut self,
        register: RegisterId,
        receiver: RegisterId,
        arguments: Vec<RegisterId>,
        location: LocationId,
    ) {
        self.instructions.push(Instruction::CallClosure(Box::new(
            CallClosure { register, receiver, arguments, location },
        )));
    }

    pub(crate) fn call_dropper(
        &mut self,
        register: RegisterId,
        receiver: RegisterId,
        location: LocationId,
    ) {
        self.instructions.push(Instruction::CallDropper(Box::new(
            CallDropper { register, receiver, location },
        )));
    }

    pub(crate) fn call_builtin(
        &mut self,
        register: RegisterId,
        name: BuiltinFunction,
        arguments: Vec<RegisterId>,
        location: LocationId,
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
        location: LocationId,
    ) {
        self.instructions.push(Instruction::Send(Box::new(Send {
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
        class: types::ClassId,
        field: types::FieldId,
        location: LocationId,
    ) {
        self.instructions.push(Instruction::GetField(Box::new(GetField {
            class,
            register,
            receiver,
            field,
            location,
        })));
    }

    pub(crate) fn set_field(
        &mut self,
        receiver: RegisterId,
        class: types::ClassId,
        field: types::FieldId,
        value: RegisterId,
        location: LocationId,
    ) {
        self.instructions.push(Instruction::SetField(Box::new(SetField {
            receiver,
            value,
            class,
            field,
            location,
        })));
    }

    pub(crate) fn pointer(
        &mut self,
        register: RegisterId,
        value: RegisterId,
        location: LocationId,
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
        class: types::ClassId,
        field: types::FieldId,
        location: LocationId,
    ) {
        self.instructions.push(Instruction::FieldPointer(Box::new(
            FieldPointer { class, register, receiver, field, location },
        )));
    }

    pub(crate) fn read_pointer(
        &mut self,
        register: RegisterId,
        pointer: RegisterId,
        location: LocationId,
    ) {
        self.instructions.push(Instruction::ReadPointer(Box::new(
            ReadPointer { register, pointer, location },
        )));
    }

    pub(crate) fn write_pointer(
        &mut self,
        pointer: RegisterId,
        value: RegisterId,
        location: LocationId,
    ) {
        self.instructions.push(Instruction::WritePointer(Box::new(
            WritePointer { pointer, value, location },
        )));
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

    pub(crate) fn spawn(
        &mut self,
        register: RegisterId,
        class: types::ClassId,
        location: LocationId,
    ) {
        self.instructions.push(Instruction::Spawn(Box::new(Spawn {
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

    pub(crate) fn reduce(&mut self, amount: u16, location: LocationId) {
        self.instructions
            .push(Instruction::Reduce(Box::new(Reduce { amount, location })))
    }

    pub(crate) fn reduce_call(&mut self, location: LocationId) {
        self.reduce(CALL_COST, location);
    }

    pub(crate) fn cast(
        &mut self,
        register: RegisterId,
        source: RegisterId,
        from: CastType,
        to: CastType,
        location: LocationId,
    ) {
        self.instructions.push(Instruction::Cast(Box::new(Cast {
            register,
            source,
            from,
            to,
            location,
        })));
    }
}

#[derive(Clone, Debug)]
pub(crate) enum Constant {
    Int(i64),
    Float(f64),
    String(Rc<String>),
    Array(Rc<Vec<Constant>>),
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
pub(crate) struct RegisterId(pub(crate) u32);

#[derive(Clone)]
pub(crate) struct Branch {
    pub(crate) condition: RegisterId,
    pub(crate) if_true: BlockId,
    pub(crate) if_false: BlockId,
    pub(crate) location: LocationId,
}

#[derive(Clone)]
pub(crate) struct Switch {
    pub(crate) register: RegisterId,
    pub(crate) blocks: Vec<BlockId>,
    pub(crate) location: LocationId,
}

#[derive(Clone)]
pub(crate) struct SwitchKind {
    pub(crate) register: RegisterId,
    pub(crate) blocks: Vec<BlockId>,
    pub(crate) location: LocationId,
}

#[derive(Clone)]
pub(crate) struct Goto {
    pub(crate) block: BlockId,
    pub(crate) location: LocationId,
}

#[derive(Clone)]
pub(crate) struct MoveRegister {
    pub(crate) source: RegisterId,
    pub(crate) target: RegisterId,
    pub(crate) location: LocationId,
}

#[derive(Clone)]
pub(crate) struct CheckRefs {
    pub(crate) register: RegisterId,
    pub(crate) location: LocationId,
}

/// Drops a value according to its type.
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
    pub(crate) register: RegisterId,
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

#[derive(Clone)]
pub(crate) struct Reference {
    pub(crate) register: RegisterId,
    pub(crate) value: RegisterId,
    pub(crate) location: LocationId,
}

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
pub(crate) struct IncrementAtomic {
    pub(crate) register: RegisterId,
    pub(crate) value: RegisterId,
    pub(crate) location: LocationId,
}

#[derive(Clone)]
pub(crate) struct DecrementAtomic {
    pub(crate) register: RegisterId,
    pub(crate) if_true: BlockId,
    pub(crate) if_false: BlockId,
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
pub(crate) struct Return {
    pub(crate) register: RegisterId,
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
pub(crate) struct CallStatic {
    pub(crate) register: RegisterId,
    pub(crate) class: types::ClassId,
    pub(crate) method: types::MethodId,
    pub(crate) arguments: Vec<RegisterId>,
    pub(crate) location: LocationId,
}

#[derive(Clone)]
pub(crate) struct CallInstance {
    pub(crate) register: RegisterId,
    pub(crate) receiver: RegisterId,
    pub(crate) method: types::MethodId,
    pub(crate) arguments: Vec<RegisterId>,
    pub(crate) location: LocationId,
}

#[derive(Clone)]
pub(crate) struct CallExtern {
    pub(crate) register: RegisterId,
    pub(crate) method: types::MethodId,
    pub(crate) arguments: Vec<RegisterId>,
    pub(crate) location: LocationId,
}

#[derive(Clone)]
pub(crate) struct CallDynamic {
    pub(crate) register: RegisterId,
    pub(crate) receiver: RegisterId,
    pub(crate) method: types::MethodId,
    pub(crate) arguments: Vec<RegisterId>,
    pub(crate) location: LocationId,
}

#[derive(Clone)]
pub(crate) struct CallClosure {
    pub(crate) register: RegisterId,
    pub(crate) receiver: RegisterId,
    pub(crate) arguments: Vec<RegisterId>,
    pub(crate) location: LocationId,
}

#[derive(Clone)]
pub(crate) struct CallBuiltin {
    pub(crate) register: RegisterId,
    pub(crate) name: BuiltinFunction,
    pub(crate) arguments: Vec<RegisterId>,
    pub(crate) location: LocationId,
}

#[derive(Clone)]
pub(crate) struct Send {
    pub(crate) receiver: RegisterId,
    pub(crate) method: types::MethodId,
    pub(crate) arguments: Vec<RegisterId>,
    pub(crate) location: LocationId,
}

#[derive(Clone)]
pub(crate) struct GetField {
    pub(crate) class: types::ClassId,
    pub(crate) register: RegisterId,
    pub(crate) receiver: RegisterId,
    pub(crate) field: types::FieldId,
    pub(crate) location: LocationId,
}

#[derive(Clone)]
pub(crate) struct SetField {
    pub(crate) class: types::ClassId,
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
pub(crate) struct Spawn {
    pub(crate) register: RegisterId,
    pub(crate) class: types::ClassId,
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

#[derive(Clone)]
pub(crate) struct Cast {
    pub(crate) register: RegisterId,
    pub(crate) source: RegisterId,
    pub(crate) from: CastType,
    pub(crate) to: CastType,
    pub(crate) location: LocationId,
}

#[derive(Clone, Debug, Copy)]
pub(crate) enum CastType {
    Int(u32),
    Float(u32),
    InkoInt,
    InkoFloat,
    Pointer,
}

#[derive(Clone, Debug, Copy)]
pub(crate) struct Pointer {
    pub(crate) register: RegisterId,
    pub(crate) value: RegisterId,
    pub(crate) location: LocationId,
}

#[derive(Clone)]
pub(crate) struct FieldPointer {
    pub(crate) class: types::ClassId,
    pub(crate) register: RegisterId,
    pub(crate) receiver: RegisterId,
    pub(crate) field: types::FieldId,
    pub(crate) location: LocationId,
}

#[derive(Clone, Debug, Copy)]
pub(crate) struct ReadPointer {
    pub(crate) register: RegisterId,
    pub(crate) pointer: RegisterId,
    pub(crate) location: LocationId,
}

#[derive(Clone, Debug, Copy)]
pub(crate) struct WritePointer {
    pub(crate) pointer: RegisterId,
    pub(crate) value: RegisterId,
    pub(crate) location: LocationId,
}

/// A MIR instruction.
///
/// When adding a new instruction that acts as an exit for a basic block, make
/// sure to also update the compiler pass that removes empty basic blocks.
#[derive(Clone)]
pub(crate) enum Instruction {
    Branch(Box<Branch>),
    Switch(Box<Switch>),
    SwitchKind(Box<SwitchKind>),
    False(Box<FalseLiteral>),
    Float(Box<FloatLiteral>),
    Goto(Box<Goto>),
    Int(Box<IntLiteral>),
    MoveRegister(Box<MoveRegister>),
    Nil(Box<NilLiteral>),
    Return(Box<Return>),
    String(Box<StringLiteral>),
    True(Box<TrueLiteral>),
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
    Clone(Box<Clone>),
    Reference(Box<Reference>),
    Increment(Box<Increment>),
    Decrement(Box<Decrement>),
    IncrementAtomic(Box<IncrementAtomic>),
    DecrementAtomic(Box<DecrementAtomic>),
    Allocate(Box<Allocate>),
    Spawn(Box<Spawn>),
    GetConstant(Box<GetConstant>),
    Reduce(Box<Reduce>),
    Finish(Box<Finish>),
    Cast(Box<Cast>),
    Pointer(Box<Pointer>),
    ReadPointer(Box<ReadPointer>),
    WritePointer(Box<WritePointer>),
    FieldPointer(Box<FieldPointer>),
}

impl Instruction {
    pub(crate) fn location(&self) -> LocationId {
        match self {
            Instruction::Branch(ref v) => v.location,
            Instruction::Switch(ref v) => v.location,
            Instruction::SwitchKind(ref v) => v.location,
            Instruction::False(ref v) => v.location,
            Instruction::True(ref v) => v.location,
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
            Instruction::Clone(ref v) => v.location,
            Instruction::Reference(ref v) => v.location,
            Instruction::Increment(ref v) => v.location,
            Instruction::Decrement(ref v) => v.location,
            Instruction::IncrementAtomic(ref v) => v.location,
            Instruction::DecrementAtomic(ref v) => v.location,
            Instruction::Allocate(ref v) => v.location,
            Instruction::Spawn(ref v) => v.location,
            Instruction::GetConstant(ref v) => v.location,
            Instruction::Reduce(ref v) => v.location,
            Instruction::Finish(ref v) => v.location,
            Instruction::Cast(ref v) => v.location,
            Instruction::Pointer(ref v) => v.location,
            Instruction::ReadPointer(ref v) => v.location,
            Instruction::WritePointer(ref v) => v.location,
            Instruction::FieldPointer(ref v) => v.location,
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
            Instruction::Switch(ref v) => {
                format!(
                    "switch r{}, {}",
                    v.register.0,
                    v.blocks
                        .iter()
                        .enumerate()
                        .map(|(idx, block)| format!("{} = b{}", idx, block.0))
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            }
            Instruction::SwitchKind(ref v) => {
                format!(
                    "switch_kind r{}, {}",
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
            Instruction::Return(ref v) => {
                format!("return r{}", v.register.0)
            }
            Instruction::Allocate(ref v) => {
                format!("r{} = allocate {}", v.register.0, v.class.name(db))
            }
            Instruction::Spawn(ref v) => {
                format!("r{} = spawn {}", v.register.0, v.class.name(db))
            }
            Instruction::CallStatic(ref v) => {
                format!(
                    "r{} = call_static {}.{}({})",
                    v.register.0,
                    v.class.name(db),
                    v.method.name(db),
                    join(&v.arguments)
                )
            }
            Instruction::CallInstance(ref v) => {
                format!(
                    "r{} = call_instance r{}.{}({})",
                    v.register.0,
                    v.receiver.0,
                    v.method.name(db),
                    join(&v.arguments)
                )
            }
            Instruction::CallExtern(ref v) => {
                format!(
                    "r{} = call_extern {}({})",
                    v.register.0,
                    v.method.name(db),
                    join(&v.arguments)
                )
            }
            Instruction::CallDynamic(ref v) => {
                format!(
                    "r{} = call_dynamic r{}.{}({})",
                    v.register.0,
                    v.receiver.0,
                    v.method.name(db),
                    join(&v.arguments)
                )
            }
            Instruction::CallClosure(ref v) => {
                format!(
                    "r{} = call_closure r{}({})",
                    v.register.0,
                    v.receiver.0,
                    join(&v.arguments)
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
                    join(&v.arguments)
                )
            }
            Instruction::Send(ref v) => {
                format!(
                    "send r{}.{}({})",
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
            Instruction::Reference(ref v) => {
                format!("r{} = ref r{}", v.register.0, v.value.0)
            }
            Instruction::Increment(ref v) => {
                format!("r{} = increment r{}", v.register.0, v.value.0)
            }
            Instruction::Decrement(ref v) => {
                format!("decrement r{}", v.register.0)
            }
            Instruction::IncrementAtomic(ref v) => {
                format!("r{} = increment_atomic r{}", v.register.0, v.value.0)
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
            Instruction::Reduce(ref v) => format!("reduce {}", v.amount),
            Instruction::Finish(v) => {
                if v.terminate { "terminate" } else { "finish" }.to_string()
            }
            Instruction::Cast(v) => {
                format!("r{} = r{} as {:?}", v.register.0, v.source.0, v.to)
            }
            Instruction::ReadPointer(v) => {
                format!("r{} = *r{}", v.register.0, v.pointer.0)
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
    pub(crate) arguments: Vec<RegisterId>,
    pub(crate) location: LocationId,
}

impl Method {
    pub(crate) fn new(id: types::MethodId, location: LocationId) -> Self {
        Self {
            id,
            body: Graph::new(),
            registers: Registers::new(),
            arguments: Vec::new(),
            location,
        }
    }
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
    locations: Vec<SourceLocation>,
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
        }
    }

    pub(crate) fn add_methods(&mut self, methods: Vec<Method>) {
        for method in methods {
            self.methods.insert(method.id, method);
        }
    }

    pub(crate) fn add_location(
        &mut self,
        location: SourceLocation,
    ) -> LocationId {
        let id = LocationId(self.locations.len());

        self.locations.push(location);
        id
    }

    pub(crate) fn last_location(&self) -> Option<LocationId> {
        if self.locations.is_empty() {
            None
        } else {
            Some(LocationId(self.locations.len() - 1))
        }
    }

    pub(crate) fn location(&self, index: LocationId) -> &SourceLocation {
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
