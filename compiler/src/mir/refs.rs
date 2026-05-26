//! MIR passes for optimizing reference and borrow counting.
use crate::mir::{
    DecrementAtomic, Goto, IncrementAtomic, Instruction, Method, MoveRegister,
    RegisterId,
};
use types::Database;

#[derive(Copy, Clone)]
enum Value {
    Unknown,
    Constant,
    Runtime(usize),
}

impl Value {
    fn is_constant(self) -> bool {
        matches!(self, Value::Constant)
    }
}

struct Values {
    map: Vec<Value>,
    id: usize,
}

impl Values {
    fn new(size: usize) -> Self {
        Self { map: vec![Value::Unknown; size], id: 0 }
    }

    fn get(&self, register: RegisterId) -> Value {
        self.map[register.0]
    }

    fn set(&mut self, register: RegisterId, value: Value) {
        self.map[register.0] = value;
    }

    fn add_runtime(&mut self, register: RegisterId) {
        self.set(register, Value::Runtime(self.id));
        self.id += 1;
    }

    fn add_constant(&mut self, register: RegisterId) {
        self.set(register, Value::Constant);
    }
}

/// Updates the list of registers to remove (`remove`) such that moved registers
/// (e.g. those written to a field) won't have their increments/decrements
/// removed.
fn retain_moved_registers(db: &Database, method: &Method, remove: &mut [bool]) {
    for block in &method.body.blocks {
        for ins in &block.instructions {
            match ins {
                Instruction::CallBuiltin(i) => {
                    i.arguments.iter().for_each(|r| remove[r.0] = false);
                }
                Instruction::CallDynamic(i) => {
                    i.arguments.iter().for_each(|r| remove[r.0] = false);
                }
                Instruction::CallStatic(i) => {
                    i.arguments.iter().for_each(|r| remove[r.0] = false);
                }
                Instruction::CallInstance(i) => {
                    if i.method.is_moving(db) {
                        remove[i.receiver.0] = false;
                    }

                    i.arguments.iter().for_each(|r| remove[r.0] = false);
                }
                Instruction::CallExtern(i) => {
                    i.arguments.iter().for_each(|r| remove[r.0] = false);
                }
                Instruction::CallClosure(i) => {
                    i.arguments.iter().for_each(|r| remove[r.0] = false);
                }
                Instruction::Send(i) => {
                    i.arguments.iter().for_each(|r| remove[r.0] = false);
                }
                Instruction::SetField(i) => remove[i.value.0] = false,
                Instruction::WritePointer(i) => remove[i.value.0] = false,
                Instruction::Return(i) => remove[i.register.0] = false,
                Instruction::Cast(i) => remove[i.source.0] = false,
                _ => {}
            }
        }
    }
}

fn replace_atomic_increment(instruction: &IncrementAtomic) -> Instruction {
    Instruction::MoveRegister(Box::new(MoveRegister {
        source: instruction.source,
        target: instruction.target,
        location: instruction.location,
        volatile: false,
    }))
}

fn replace_atomic_decrement(instruction: &DecrementAtomic) -> Instruction {
    Instruction::Goto(Box::new(Goto {
        block: instruction.if_false,
        location: instruction.location,
    }))
}

fn instruction_target(instruction: &Instruction) -> Option<(bool, RegisterId)> {
    match instruction {
        Instruction::String(i) => Some((true, i.register)),
        Instruction::GetConstant(i) => Some((true, i.register)),
        Instruction::Cast(i) => Some((false, i.register)),
        Instruction::GetField(i) => Some((false, i.register)),
        Instruction::ReadPointer(i) => Some((false, i.register)),
        Instruction::CallBuiltin(i) => Some((false, i.register)),
        Instruction::CallInstance(i) => Some((false, i.register)),
        Instruction::CallStatic(i) => Some((false, i.register)),
        Instruction::CallDynamic(i) => Some((false, i.register)),
        Instruction::CallClosure(i) => Some((false, i.register)),
        Instruction::CallExtern(i) => Some((false, i.register)),
        _ => None,
    }
}

pub(crate) fn remove_redundant_reference_counts(
    db: &Database,
    method: &mut Method,
) {
    let reconnect = remove_constant_string_reference_counts(db, method)
        | remove_string_argument_reference_counts(db, method);

    if reconnect {
        method.reconnect_blocks();
        method.remove_unreachable_blocks();
    }
}

/// Removes all reference counting operations for string constants and literals.
///
/// These strings are never deallocated and thus reference counting related
/// operations for these values are redundant.
fn remove_constant_string_reference_counts(
    db: &Database,
    method: &mut Method,
) -> bool {
    let mut values = Values::new(method.registers.len());

    // We don't know if arguments are given constants or runtime values, so we
    // have to be pessimistic and assume they're always runtime values.
    for &reg in &method.arguments {
        if method.registers.value_type(reg).is_string(db) {
            values.add_runtime(reg);
        }
    }

    for block in &method.body.blocks {
        for ins in &block.instructions {
            let Some((cons, reg)) = instruction_target(ins) else { continue };

            if !method.registers.value_type(reg).is_string(db) {
                continue;
            }

            if cons {
                values.add_constant(reg);
            } else {
                values.add_runtime(reg);
            }
        }
    }

    // If a register is the target for multiple moves (= the result of a
    // `match` for example) we essentially treat the register as containing
    // a unique runtime string, unless all sources are constant strings.
    let mut merged = vec![false; method.registers.len()];
    let mut run = true;

    // It's possible the graph is constructed such that we can't propagate
    // the values across registers in a single iteration, regardless of what
    // order we iterate in. As such we keep iterating until we run out of
    // registers to update.
    //
    // Testing using a few Inko applications (e.g. shost) shows that in most
    // cases no more than 2-5 iterations are necessary.
    while run {
        run = false;

        // We iterate in breadth-first order to reduce the amount of iterations
        // necessary per method.
        for bid in method.body.iter() {
            for ins in &method.body.block(bid).instructions {
                let (src, reg) = match ins {
                    Instruction::MoveRegister(i) => (i.source, i.target),
                    Instruction::IncrementAtomic(i) => (i.source, i.target),
                    _ => continue,
                };

                match (values.get(reg), values.get(src)) {
                    (Value::Unknown, Value::Unknown) => {
                        // No point in propagating unknown values.
                    }
                    (Value::Unknown, val) => {
                        // The first time a register is set we inherit the
                        // value.
                        values.set(reg, val);
                        run = true;
                    }
                    (Value::Constant, Value::Constant) => {
                        // Constants are kept as-is so we can remove their
                        // ref counts as much as possible.
                    }
                    (Value::Runtime(a), Value::Runtime(b)) if a == b => {
                        // This happens if we visit the same assignment on a
                        // future iteration. In this case we keep the value
                        // as-is
                    }
                    // let mut a = 'string literal'
                    // a = runtime_string
                    //
                    // let mut a = runtime_string
                    // a = 'string literal'
                    (_, Value::Runtime(_)) | (_, Value::Constant)
                        if !merged[reg.0] =>
                    {
                        // In this case we treat the value as a _new_
                        // unrelated string so we don't end up removing the
                        // wrong ref counts due to a branch.
                        merged[reg.0] = true;
                        values.add_runtime(reg);
                        run = true;
                    }
                    _ => {}
                }
            }
        }
    }

    let mut reconnect = false;

    for block in &mut method.body.blocks {
        for ins in &mut block.instructions {
            match ins {
                Instruction::IncrementAtomic(i)
                    if values.get(i.source).is_constant() =>
                {
                    *ins = replace_atomic_increment(i);
                }
                Instruction::DecrementAtomic(i)
                    if values.get(i.register).is_constant() =>
                {
                    reconnect = true;
                    *ins = replace_atomic_decrement(i);
                }
                Instruction::Free(i)
                    if values.get(i.register).is_constant() =>
                {
                    *ins = Instruction::Nop(i.location);
                }
                _ => {}
            }
        }
    }

    reconnect
}

/// Removes redundant reference counts for string arguments passed to methods
/// that are now inlined.
///
/// When a method is called and given a string argument, an increment is
/// generated by the caller such that this:
///
/// ```
/// function(string)
/// ```
///
/// Results in roughly the following:
///
/// ```
/// incr string
/// function(string)
/// ```
///
/// The callee is then responsible for dropping the argument at the end of the
/// scope.
///
/// After such a method is inlined the increment and its corresponding decrement
/// are redundant. This function looks for such pairs and removes them.
fn remove_string_argument_reference_counts(
    db: &Database,
    method: &mut Method,
) -> bool {
    let mut reconnect = false;
    let mut remove = vec![false; method.registers.len()];

    for block in &method.body.blocks {
        for ins in &block.instructions {
            let Instruction::IncrementAtomic(i) = ins else { continue };

            if method.registers.is_argument(i.target) {
                remove[i.target.0] = true;
            }
        }
    }

    // If the register escapes we need to retain its increment, such as when
    // a method that stores an argument in a field is inlined.
    retain_moved_registers(db, method, &mut remove);

    for block in &mut method.body.blocks {
        for ins in &mut block.instructions {
            match ins {
                Instruction::IncrementAtomic(i) if remove[i.target.0] => {
                    *ins = replace_atomic_increment(i);
                }
                Instruction::DecrementAtomic(i) if remove[i.register.0] => {
                    reconnect = true;
                    *ins = replace_atomic_decrement(i);
                }
                _ => continue,
            }
        }
    }

    reconnect
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mir::{
        self, CastType, Graph, InstructionLocation, Method, Registers,
    };
    use location::Location;
    use types::{
        ConstantId, Database, FieldId, Intrinsic, Method as MethodType,
        MethodId, MethodKind, ModuleId, TypeId, TypeRef, Visibility,
    };

    fn loc() -> InstructionLocation {
        InstructionLocation { line: 1, column: 1, inlined_call_id: 0 }
    }

    #[test]
    fn test_values_set_get() {
        let mut vals = Values::new(4);

        vals.set(RegisterId(2), Value::Constant);
        vals.set(RegisterId(3), Value::Runtime(2));

        assert!(matches!(vals.get(RegisterId(1)), Value::Unknown));
        assert!(matches!(vals.get(RegisterId(2)), Value::Constant));
        assert!(matches!(vals.get(RegisterId(3)), Value::Runtime(2)));
    }

    #[test]
    fn test_values_add_runtime() {
        let mut vals = Values::new(4);

        vals.add_runtime(RegisterId(1));
        vals.add_runtime(RegisterId(2));

        assert!(matches!(vals.get(RegisterId(1)), Value::Runtime(0)));
        assert!(matches!(vals.get(RegisterId(2)), Value::Runtime(1)));
    }

    #[test]
    fn test_values_add_constant() {
        let mut vals = Values::new(4);

        vals.add_constant(RegisterId(1));
        assert!(matches!(vals.get(RegisterId(1)), Value::Constant));
    }

    #[test]
    fn test_retain_moved_registers() {
        let mut db = Database::new();
        let mloc = Location {
            line_start: 1,
            line_end: 1,
            column_start: 1,
            column_end: 1,
        };
        let ins_meth = MethodType::alloc(
            &mut db,
            ModuleId(0),
            mloc,
            "a".to_string(),
            Visibility::Public,
            MethodKind::Instance,
        );
        let mov_meth = MethodType::alloc(
            &mut db,
            ModuleId(0),
            mloc,
            "a".to_string(),
            Visibility::Public,
            MethodKind::Moving,
        );
        let mut meth = Method {
            id: MethodId(0),
            registers: Registers::new(),
            body: Graph::new(),
            arguments: Vec::new(),
            inlined_calls: Vec::new(),
        };
        let block = meth.body.add_block();
        let tests = vec![
            (
                Instruction::CallBuiltin(Box::new(mir::CallBuiltin {
                    register: RegisterId(0),
                    name: Intrinsic::Memset,
                    arguments: vec![RegisterId(1)],
                    location: loc(),
                })),
                vec![true, false],
            ),
            (
                Instruction::CallDynamic(Box::new(mir::CallDynamic {
                    register: RegisterId(0),
                    receiver: RegisterId(1),
                    method: MethodId(0),
                    arguments: vec![RegisterId(2)],
                    type_arguments: None,
                    location: loc(),
                })),
                vec![true, true, false],
            ),
            (
                Instruction::CallStatic(Box::new(mir::CallStatic {
                    register: RegisterId(0),
                    method: MethodId(0),
                    arguments: vec![RegisterId(1)],
                    type_arguments: None,
                    location: loc(),
                })),
                vec![true, false],
            ),
            (
                Instruction::CallInstance(Box::new(mir::CallInstance {
                    register: RegisterId(0),
                    receiver: RegisterId(1),
                    method: ins_meth,
                    arguments: vec![RegisterId(2)],
                    type_arguments: None,
                    location: loc(),
                })),
                vec![true, true, false],
            ),
            (
                Instruction::CallInstance(Box::new(mir::CallInstance {
                    register: RegisterId(0),
                    receiver: RegisterId(1),
                    method: mov_meth,
                    arguments: vec![RegisterId(2)],
                    type_arguments: None,
                    location: loc(),
                })),
                vec![true, false, false],
            ),
            (
                Instruction::CallExtern(Box::new(mir::CallExtern {
                    register: RegisterId(0),
                    method: MethodId(0),
                    arguments: vec![RegisterId(1)],
                    location: loc(),
                })),
                vec![true, false],
            ),
            (
                Instruction::CallClosure(Box::new(mir::CallClosure {
                    register: RegisterId(0),
                    receiver: RegisterId(1),
                    arguments: vec![RegisterId(2)],
                    location: loc(),
                })),
                vec![true, true, false],
            ),
            (
                Instruction::Send(Box::new(mir::Send {
                    receiver: RegisterId(1),
                    method: MethodId(0),
                    arguments: vec![RegisterId(1)],
                    type_arguments: None,
                    location: loc(),
                })),
                vec![true, false],
            ),
            (
                Instruction::SetField(Box::new(mir::SetField {
                    type_id: TypeId(0),
                    receiver: RegisterId(0),
                    value: RegisterId(1),
                    field: FieldId(0),
                    location: loc(),
                })),
                vec![true, false],
            ),
            (
                Instruction::WritePointer(Box::new(mir::WritePointer {
                    pointer: RegisterId(0),
                    value: RegisterId(1),
                    location: loc(),
                })),
                vec![true, false],
            ),
            (
                Instruction::Return(Box::new(mir::Return {
                    register: RegisterId(0),
                    location: loc(),
                })),
                vec![false],
            ),
            (
                Instruction::Cast(Box::new(mir::Cast {
                    register: RegisterId(0),
                    source: RegisterId(1),
                    from: CastType::Object,
                    to: CastType::Object,
                    location: loc(),
                })),
                vec![true, false],
            ),
        ];

        for (ins, exp) in tests {
            let mut remove = vec![true; exp.len()];

            meth.body.block_mut(block).instructions.clear();
            meth.body.block_mut(block).instructions.push(ins);
            retain_moved_registers(&db, &meth, &mut remove);

            assert_eq!(remove, exp);
        }
    }

    #[test]
    fn test_instruction_target() {
        let tests = vec![
            (
                Instruction::CallBuiltin(Box::new(mir::CallBuiltin {
                    register: RegisterId(0),
                    name: Intrinsic::Memset,
                    arguments: Vec::new(),
                    location: loc(),
                })),
                Some((false, RegisterId(0))),
            ),
            (
                Instruction::CallDynamic(Box::new(mir::CallDynamic {
                    register: RegisterId(0),
                    receiver: RegisterId(1),
                    method: MethodId(0),
                    arguments: Vec::new(),
                    type_arguments: None,
                    location: loc(),
                })),
                Some((false, RegisterId(0))),
            ),
            (
                Instruction::CallStatic(Box::new(mir::CallStatic {
                    register: RegisterId(0),
                    method: MethodId(0),
                    arguments: Vec::new(),
                    type_arguments: None,
                    location: loc(),
                })),
                Some((false, RegisterId(0))),
            ),
            (
                Instruction::CallInstance(Box::new(mir::CallInstance {
                    register: RegisterId(0),
                    receiver: RegisterId(1),
                    method: MethodId(0),
                    arguments: Vec::new(),
                    type_arguments: None,
                    location: loc(),
                })),
                Some((false, RegisterId(0))),
            ),
            (
                Instruction::CallExtern(Box::new(mir::CallExtern {
                    register: RegisterId(0),
                    method: MethodId(0),
                    arguments: Vec::new(),
                    location: loc(),
                })),
                Some((false, RegisterId(0))),
            ),
            (
                Instruction::CallClosure(Box::new(mir::CallClosure {
                    register: RegisterId(0),
                    receiver: RegisterId(1),
                    arguments: Vec::new(),
                    location: loc(),
                })),
                Some((false, RegisterId(0))),
            ),
            (
                Instruction::GetField(Box::new(mir::GetField {
                    type_id: TypeId(0),
                    register: RegisterId(0),
                    receiver: RegisterId(1),
                    field: FieldId(0),
                    location: loc(),
                })),
                Some((false, RegisterId(0))),
            ),
            (
                Instruction::ReadPointer(Box::new(mir::ReadPointer {
                    register: RegisterId(0),
                    pointer: RegisterId(1),
                    location: loc(),
                })),
                Some((false, RegisterId(0))),
            ),
            (
                Instruction::Cast(Box::new(mir::Cast {
                    register: RegisterId(0),
                    source: RegisterId(1),
                    from: CastType::Object,
                    to: CastType::Object,
                    location: loc(),
                })),
                Some((false, RegisterId(0))),
            ),
            (
                Instruction::String(Box::new(mir::StringLiteral {
                    register: RegisterId(0),
                    value: "a".to_string(),
                    location: loc(),
                })),
                Some((true, RegisterId(0))),
            ),
            (
                Instruction::GetConstant(Box::new(mir::GetConstant {
                    register: RegisterId(0),
                    id: ConstantId(0),
                    location: loc(),
                })),
                Some((true, RegisterId(0))),
            ),
            (
                Instruction::Int(Box::new(mir::IntLiteral {
                    register: RegisterId(0),
                    value: 10,
                    bits: 64,
                    location: loc(),
                })),
                None,
            ),
        ];

        for (ins, exp) in tests {
            assert_eq!(instruction_target(&ins), exp);
        }
    }

    #[test]
    fn test_remove_redundant_reference_counts() {
        let db = Database::new();
        let mut meth = Method {
            id: MethodId(0),
            registers: Registers::new(),
            body: Graph::new(),
            arguments: Vec::new(),
            inlined_calls: Vec::new(),
        };
        let blk = meth.body.add_block();
        let del = meth.body.add_block();
        let ret = meth.body.add_block();
        let val = meth.registers.alloc(TypeRef::string());

        meth.add_argument(val);
        meth.body.block_mut(blk).increment_atomic(val, val, loc());
        meth.body.block_mut(blk).decrement_atomic(val, del, ret, loc());

        remove_redundant_reference_counts(&db, &mut meth);

        assert_eq!(meth.body.blocks.len(), 2);

        let ins = &meth.body.block(blk).instructions;

        assert!(matches!(ins[0], Instruction::MoveRegister(_)));
        assert!(matches!(ins[1], Instruction::Goto(_)));
    }

    #[test]
    fn test_remove_constant_string_reference_counts_with_constant_strings() {
        let db = Database::new();
        let mut meth = Method {
            id: MethodId(0),
            registers: Registers::new(),
            body: Graph::new(),
            arguments: Vec::new(),
            inlined_calls: Vec::new(),
        };
        let blk = meth.body.add_block();
        let del = meth.body.add_block();
        let ret = meth.body.add_block();
        let val = meth.registers.alloc(TypeRef::string());

        meth.body.block_mut(blk).string_literal(val, "a".to_string(), loc());
        meth.body.block_mut(blk).increment_atomic(val, val, loc());
        meth.body.block_mut(blk).decrement_atomic(val, del, ret, loc());
        meth.body.block_mut(del).free(val, TypeId(0), loc());
        meth.body.block_mut(del).goto(ret, loc());

        assert!(remove_constant_string_reference_counts(&db, &mut meth));
        assert!(matches!(
            meth.body.block(del).instructions[0],
            Instruction::Nop(_)
        ));

        let ins = &meth.body.block(blk).instructions;

        assert!(matches!(ins[1], Instruction::MoveRegister(_)));
        assert!(matches!(ins[2], Instruction::Goto(_)));
    }

    #[test]
    fn test_remove_constant_string_reference_counts_with_runtime_strings() {
        let db = Database::new();
        let mut meth = Method {
            id: MethodId(0),
            registers: Registers::new(),
            body: Graph::new(),
            arguments: Vec::new(),
            inlined_calls: Vec::new(),
        };
        let blk = meth.body.add_block();
        let del = meth.body.add_block();
        let ret = meth.body.add_block();
        let val = meth.registers.alloc(TypeRef::string());

        meth.body.block_mut(blk).get_field(
            val,
            val,
            TypeId(0),
            FieldId(0),
            loc(),
        );
        meth.body.block_mut(blk).increment_atomic(val, val, loc());
        meth.body.block_mut(blk).decrement_atomic(val, del, ret, loc());
        meth.body.block_mut(del).free(val, TypeId(0), loc());
        meth.body.block_mut(del).goto(ret, loc());

        assert!(!remove_constant_string_reference_counts(&db, &mut meth));
        assert!(matches!(
            meth.body.block(del).instructions[0],
            Instruction::Free(_)
        ));

        let ins = &meth.body.block(blk).instructions;

        assert!(matches!(ins[1], Instruction::IncrementAtomic(_)));
        assert!(matches!(ins[2], Instruction::DecrementAtomic(_)));
    }

    #[test]
    fn test_remove_constant_string_reference_counts_with_mixed_strings() {
        let db = Database::new();
        let mut meth = Method {
            id: MethodId(0),
            registers: Registers::new(),
            body: Graph::new(),
            arguments: Vec::new(),
            inlined_calls: Vec::new(),
        };
        let blk = meth.body.add_block();
        let del = meth.body.add_block();
        let ret = meth.body.add_block();
        let val = meth.registers.alloc(TypeRef::string());

        meth.body.block_mut(blk).string_literal(val, "a".to_string(), loc());
        meth.body.block_mut(blk).get_field(
            val,
            val,
            TypeId(0),
            FieldId(0),
            loc(),
        );
        meth.body.block_mut(blk).increment_atomic(val, val, loc());
        meth.body.block_mut(blk).decrement_atomic(val, del, ret, loc());
        meth.body.block_mut(del).free(val, TypeId(0), loc());
        meth.body.block_mut(del).goto(ret, loc());

        assert!(!remove_constant_string_reference_counts(&db, &mut meth));
        assert!(matches!(
            meth.body.block(del).instructions[0],
            Instruction::Free(_)
        ));

        let ins = &meth.body.block(blk).instructions;

        assert!(matches!(ins[0], Instruction::String(_)));
        assert!(matches!(ins[1], Instruction::GetField(_)));
        assert!(matches!(ins[2], Instruction::IncrementAtomic(_)));
        assert!(matches!(ins[3], Instruction::DecrementAtomic(_)));
    }

    #[test]
    fn test_remove_constant_string_reference_counts_with_arguments() {
        let db = Database::new();
        let mut meth = Method {
            id: MethodId(0),
            registers: Registers::new(),
            body: Graph::new(),
            arguments: Vec::new(),
            inlined_calls: Vec::new(),
        };
        let blk = meth.body.add_block();
        let del = meth.body.add_block();
        let ret = meth.body.add_block();
        let val = meth.registers.alloc(TypeRef::string());

        meth.add_argument(val);

        meth.body.block_mut(blk).increment_atomic(val, val, loc());
        meth.body.block_mut(blk).decrement_atomic(val, del, ret, loc());
        meth.body.block_mut(del).free(val, TypeId(0), loc());
        meth.body.block_mut(del).goto(ret, loc());

        assert!(!remove_constant_string_reference_counts(&db, &mut meth));
        assert!(matches!(
            meth.body.block(del).instructions[0],
            Instruction::Free(_)
        ));

        let ins = &meth.body.block(blk).instructions;

        assert!(matches!(ins[0], Instruction::IncrementAtomic(_)));
        assert!(matches!(ins[1], Instruction::DecrementAtomic(_)));
    }

    #[test]
    fn test_remove_string_argument_reference_counts() {
        let db = Database::new();
        let mut meth = Method {
            id: MethodId(0),
            registers: Registers::new(),
            body: Graph::new(),
            arguments: Vec::new(),
            inlined_calls: Vec::new(),
        };
        let blk = meth.body.add_block();
        let del = meth.body.add_block();
        let ret = meth.body.add_block();
        let val = meth.registers.alloc(TypeRef::string());

        meth.add_argument(val);
        meth.body.block_mut(blk).increment_atomic(val, val, loc());
        meth.body.block_mut(blk).decrement_atomic(val, del, ret, loc());

        assert!(remove_string_argument_reference_counts(&db, &mut meth));

        let ins = &meth.body.block(blk).instructions;

        assert!(matches!(ins[0], Instruction::MoveRegister(_)));
        assert!(matches!(ins[1], Instruction::Goto(_)));
    }

    #[test]
    fn test_remove_string_argument_reference_counts_with_moved_register() {
        let db = Database::new();
        let mut meth = Method {
            id: MethodId(0),
            registers: Registers::new(),
            body: Graph::new(),
            arguments: Vec::new(),
            inlined_calls: Vec::new(),
        };
        let blk = meth.body.add_block();
        let del = meth.body.add_block();
        let ret = meth.body.add_block();
        let val = meth.registers.alloc(TypeRef::string());

        meth.add_argument(val);
        meth.body.block_mut(blk).increment_atomic(val, val, loc());
        meth.body.block_mut(blk).write_pointer(val, val, loc());
        meth.body.block_mut(blk).decrement_atomic(val, del, ret, loc());

        assert!(!remove_string_argument_reference_counts(&db, &mut meth));

        let ins = &meth.body.block(blk).instructions;

        assert!(matches!(ins[0], Instruction::IncrementAtomic(_)));
        assert!(matches!(ins[2], Instruction::DecrementAtomic(_)));
    }
}
