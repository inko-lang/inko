//! Converting MIR to bytecode.
use crate::mir::{self, CloneKind, Constant, LocationId, Method, Mir};
use bytecode::{Instruction, Opcode};
use bytecode::{
    CONST_FLOAT, CONST_INTEGER, CONST_STRING, SIGNATURE_BYTES, VERSION,
};
use std::collections::{HashMap, HashSet, VecDeque};
use types::{ClassId, Database, MethodId, FIRST_USER_CLASS_ID};

const REGISTERS_LIMIT: usize = u16::MAX as usize;
const BLOCKS_LIMIT: usize = u16::MAX as usize;
const JUMP_TABLES_LIMIT: usize = u16::MAX as usize;

/// Method table sizes are multiplied by this value in an attempt to reduce the
/// amount of collisions when performing dynamic dispatch.
///
/// While this increases the amount of memory needed per method table, it's not
/// really significant: each slot only takes up one word of memory. On a 64-bits
/// system this means you can fit a total of 131 072 slots in 1 MiB. In
/// addition, this cost is a one-time and constant cost, whereas collisions
/// introduce a cost that you may have to pay every time you perform dynamic
/// dispatch.
const METHOD_TABLE_FACTOR: usize = 4;

/// The class-local index of the dropper method.
const DROPPER_INDEX: u16 = 0;

/// The class-local index of the "call" method for closures.
const CALL_CLOSURE_INDEX: u16 = 1;

fn nearest_power_of_two(mut value: usize) -> usize {
    if value == 0 {
        return 0;
    }

    value -= 1;
    value |= value >> 1;
    value |= value >> 2;
    value |= value >> 4;
    value |= value >> 8;
    value |= value >> 16;
    value |= value >> 32;
    value += 1;

    value
}

fn split_u32(value: u32) -> (u16, u16) {
    let bytes = u32::to_le_bytes(value);

    (
        u16::from_le_bytes([bytes[0], bytes[1]]),
        u16::from_le_bytes([bytes[2], bytes[3]]),
    )
}

fn push_values(buffer: &mut Vec<Instruction>, values: &[mir::RegisterId]) {
    for val in values {
        buffer.push(Instruction::one(Opcode::Push, val.0 as u16));
    }
}

fn number_of_methods(info: &HashMap<ClassId, ClassInfo>, id: ClassId) -> u16 {
    info.get(&id).unwrap().method_slots
}

pub(crate) struct Bytecode {
    pub(crate) bytes: Vec<u8>,
}

pub(crate) struct Buffer {
    pub(crate) bytes: Vec<u8>,
}

impl Buffer {
    fn new() -> Self {
        Self { bytes: Vec::new() }
    }

    fn len(&self) -> usize {
        self.bytes.len()
    }

    fn append(&mut self, other: &mut Self) {
        self.bytes.append(&mut other.bytes);
    }

    fn write_u8(&mut self, value: u8) {
        self.bytes.push(value);
    }

    fn write_bool(&mut self, value: bool) {
        self.bytes.push(value as u8);
    }

    fn write_u16(&mut self, value: u16) {
        self.bytes.extend_from_slice(&u16::to_le_bytes(value));
    }

    fn write_u32(&mut self, value: u32) {
        self.bytes.extend_from_slice(&u32::to_le_bytes(value));
    }

    fn write_u64(&mut self, value: u64) {
        self.bytes.extend_from_slice(&u64::to_le_bytes(value));
    }

    fn write_i64(&mut self, value: i64) {
        self.bytes.extend_from_slice(&i64::to_le_bytes(value));
    }

    fn write_f64(&mut self, value: f64) {
        self.bytes.extend_from_slice(&u64::to_le_bytes(value.to_bits()));
    }

    fn write_string(&mut self, value: &str) {
        self.write_u32(value.len() as u32);
        self.bytes.extend_from_slice(value.as_bytes());
    }
}

struct ClassInfo {
    /// A globally unique, monotonically increasing index for this class.
    ///
    /// MIR may remove classes as part of certain optimisations, so the class
    /// IDs may not be monotonically increasing. The VM expects a list of
    /// classes and IDs, without any holes between them. To achieve this we
    /// assign every class ID a monotonically increasing index, and use that
    /// index when generating bytecode.
    index: u32,

    /// The number of slots to reserve for methods.
    method_slots: u16,

    /// The number of fields the class defines.
    fields: u8,
}

struct MethodInfo {
    index: u16,
    hash: u32,
}

/// A compiler pass that lowers MIR into a bytecode file.
pub(crate) struct Lower<'a> {
    db: &'a Database,
    mir: &'a Mir,
    class_info: &'a HashMap<ClassId, ClassInfo>,
    method_info: &'a HashMap<MethodId, MethodInfo>,
    constant_indexes: HashMap<Constant, u32>,
}

impl<'a> Lower<'a> {
    pub(crate) fn run_all(db: &'a Database, mir: Mir) -> Bytecode {
        let mut method_hashes: HashMap<&str, u32> = HashMap::new();
        let mut method_info = HashMap::new();
        let mut class_info = HashMap::new();
        let mut class_index = FIRST_USER_CLASS_ID;

        // These methods are given fixed hash codes so they always reside in a
        // fixed slot, removing the need for dynamic dispatch when calling these
        // methods.
        method_hashes.insert(types::DROPPER_METHOD, DROPPER_INDEX as u32);
        method_hashes.insert(types::CALL_METHOD, CALL_CLOSURE_INDEX as u32);

        for (&class_id, class) in &mir.classes {
            let index = if class_id.0 < FIRST_USER_CLASS_ID {
                class_id.0
            } else {
                let idx = class_index;

                class_index += 1;
                idx
            };

            let num_methods = class_id.number_of_methods(db);

            // For classes with very few methods, the cost of handling
            // collisions is so low we can just keep the table sizes as small as
            // possible.
            let raw_size = if num_methods <= 4 {
                num_methods
            } else {
                num_methods * METHOD_TABLE_FACTOR
            };

            // The number of methods is a power of two, as this allows the use
            // of the & operator instead of the % operator. The & operator
            // requires far fewer instructions compared to the the % operator.
            let size = nearest_power_of_two(raw_size);
            let mut buckets = vec![false; size];

            for &method_id in &class.methods {
                // We use a state of the art hashing algorithm (patent pending):
                // each unique name is simply assigned a monotonically
                // increasing 32-bits unsigned integer. This is safe because as
                // part of type-checking we limit the total number of methods to
                // fit in this number.
                //
                // Indexes can be safely cast to a u16 because we limit the
                // number of methods per class to fit in this limit.
                let next_hash = method_hashes.len() as u32;
                let hash = *method_hashes
                    .entry(method_id.name(db))
                    .or_insert(next_hash);

                // Because the number of methods/buckets is a power of two, we
                // can also use the & operator here. The subtractions of 1 are
                // needed to ensure the & operator works correctly:
                //
                //     15 % 8       => 7
                //     15 & 8       => 8
                //     15 & (8 - 1) => 7
                let raw_index = hash as usize & (size - 1);
                let mut index = raw_index;

                while buckets[index] {
                    index = (index + 1) & (buckets.len() - 1);
                }

                buckets[index] = true;

                let info = MethodInfo { index: index as u16, hash };

                method_info.insert(method_id, info);
            }

            let info = ClassInfo {
                index,
                method_slots: size as u16,
                fields: class_id.number_of_fields(db) as u8,
            };

            class_info.insert(class_id, info);
        }

        for mir_trait in mir.traits.values() {
            for method_id in mir_trait
                .id
                .required_methods(db)
                .into_iter()
                .chain(mir_trait.id.default_methods(db))
            {
                let next_hash = method_hashes.len() as u32;
                let hash = *method_hashes
                    .entry(method_id.name(db))
                    .or_insert(next_hash);

                method_info.insert(method_id, MethodInfo { index: 0, hash });
            }
        }

        let mut buffer = Buffer::new();
        let main_mod = db.module(db.main_module().unwrap().as_str());
        let main_class = match main_mod.symbol(db, types::MAIN_CLASS) {
            Some(types::Symbol::Class(id)) => id,
            _ => unreachable!(),
        };
        let main_method = main_class.method(db, types::MAIN_METHOD).unwrap();
        let main_class_idx = class_info.get(&main_class).unwrap().index;
        let main_method_idx = method_info.get(&main_method).unwrap().index;

        buffer.bytes.extend_from_slice(&SIGNATURE_BYTES);
        buffer.write_u8(VERSION);
        buffer.write_u32(mir.modules.len() as u32);
        buffer.write_u32(mir.classes.len() as u32);

        buffer.write_u16(number_of_methods(&class_info, ClassId::int()));
        buffer.write_u16(number_of_methods(&class_info, ClassId::float()));
        buffer.write_u16(number_of_methods(&class_info, ClassId::string()));
        buffer.write_u16(number_of_methods(&class_info, ClassId::array()));
        buffer.write_u16(number_of_methods(&class_info, ClassId::boolean()));
        buffer.write_u16(number_of_methods(&class_info, ClassId::nil()));
        buffer.write_u16(number_of_methods(&class_info, ClassId::byte_array()));
        buffer.write_u16(number_of_methods(&class_info, ClassId::future()));

        buffer.write_u32(main_class_idx);
        buffer.write_u16(main_method_idx);

        for index in 0..mir.modules.len() {
            let mut chunk = Lower {
                db,
                mir: &mir,
                constant_indexes: HashMap::new(),
                class_info: &class_info,
                method_info: &method_info,
            }
            .run(index);

            buffer.write_u64(chunk.len() as u64);
            buffer.append(&mut chunk);
        }

        Bytecode { bytes: buffer.bytes }
    }

    pub(crate) fn run(mut self, module_index: usize) -> Buffer {
        let mir_mod = self.mir.modules[module_index].clone();

        for id in mir_mod.constants {
            let val = self.mir.constants.get(&id).cloned().unwrap();

            self.constant_index(val);
        }

        let mut classes_buffer = Buffer::new();

        classes_buffer.write_u16(mir_mod.classes.len() as u16);

        for class_id in mir_mod.classes {
            self.class(class_id, &mut classes_buffer);
        }

        let mut module_buffer = Buffer::new();

        module_buffer.write_u32(module_index as u32);

        // We currently don't validate the number of constants, based on the
        // idea that you are _extremely_ unlikely to ever need more than
        // (2^32)-1 constants in a single module.
        module_buffer.write_u32(self.constant_indexes.len() as u32);

        for (cons, index) in self.constant_indexes {
            module_buffer.write_u32(index);

            match cons {
                Constant::Int(v) => {
                    module_buffer.write_u8(CONST_INTEGER);
                    module_buffer.write_i64(v);
                }
                Constant::Float(v) => {
                    module_buffer.write_u8(CONST_FLOAT);
                    module_buffer.write_f64(v);
                }
                Constant::String(v) => {
                    module_buffer.write_u8(CONST_STRING);
                    module_buffer.write_string(&v);
                }
            }
        }

        let class_idx =
            self.class_info.get(&mir_mod.id.class(self.db)).unwrap().index;

        module_buffer.write_u32(class_idx);
        module_buffer.append(&mut classes_buffer);
        module_buffer
    }

    fn class(&mut self, class_id: ClassId, buffer: &mut Buffer) {
        let info = self.class_info.get(&class_id).unwrap();
        let mir_class = self.mir.classes.get(&class_id).unwrap();

        buffer.write_u32(info.index);
        buffer.write_bool(class_id.kind(self.db).is_async());
        buffer.write_string(class_id.name(self.db));
        buffer.write_u8(info.fields);
        buffer.write_u16(info.method_slots);
        buffer.write_u16(mir_class.methods.len() as u16);

        for &mid in &mir_class.methods {
            self.method(mid, buffer);
        }
    }

    fn method(&mut self, method_id: MethodId, buffer: &mut Buffer) {
        let info = self.method_info.get(&method_id).unwrap();
        let method = self.mir.methods.get(&method_id).unwrap();
        let regs = method.registers.len();
        let mut jump_tables = Vec::new();
        let mut locations = Vec::new();

        // This should never happen, unless somebody is writing _really_
        // crazy code, or due to a compiler bug. Because of this, we just
        // resort to a simple assertion.
        assert!(regs <= REGISTERS_LIMIT);
        assert!(method.body.blocks.len() <= BLOCKS_LIMIT);

        buffer.write_u16(info.index);
        buffer.write_u32(info.hash);
        buffer.write_u16(regs as u16);

        self.instructions(method, buffer, &mut jump_tables, &mut locations);
        self.location_table(buffer, locations);

        assert!(jump_tables.len() <= JUMP_TABLES_LIMIT);

        buffer.write_u16(jump_tables.len() as u16);

        for table in jump_tables {
            // MIR already limits the possible switch (and thus jump table
            // values), e.g. by limiting the number of enum variants.
            buffer.write_u16(table.len() as u16);

            for value in table {
                buffer.write_u32(value);
            }
        }
    }

    fn instructions(
        &mut self,
        method: &Method,
        buffer: &mut Buffer,
        jump_tables: &mut Vec<Vec<u32>>,
        locations: &mut Vec<(usize, LocationId)>,
    ) {
        let mut instructions = Vec::new();
        let mut queue = VecDeque::new();
        let mut visited = HashSet::new();
        let mut offsets = vec![0_u16; method.body.blocks.len()];

        // Arguments are pushed in order, meaning that for arguments `(a, b, c)`
        // the stack would be `[a, b, c]`, and thus the first pop produces the
        // last argument.
        for index in (0..=method.id.number_of_arguments(self.db)).rev() {
            instructions.push(Instruction::one(Opcode::Pop, index as u16));
        }

        queue.push_back(method.body.start_id);
        visited.insert(method.body.start_id);

        while let Some(block_id) = queue.pop_front() {
            let block = &method.body.blocks[block_id.0];
            let offset = instructions.len() as u16;

            offsets[block_id.0] = offset;

            for ins in &block.instructions {
                self.instruction(
                    method,
                    ins,
                    &mut instructions,
                    jump_tables,
                    locations,
                );
            }

            for &child in &block.successors {
                if visited.insert(child) {
                    queue.push_back(child);
                }
            }
        }

        for ins in &mut instructions {
            match ins.opcode {
                Opcode::Goto => {
                    ins.arguments[0] = offsets[ins.arg(0) as usize];
                }
                Opcode::Branch => {
                    ins.arguments[1] = offsets[ins.arg(1) as usize];
                    ins.arguments[2] = offsets[ins.arg(2) as usize];
                }
                Opcode::BranchResult => {
                    ins.arguments[0] = offsets[ins.arg(0) as usize];
                    ins.arguments[1] = offsets[ins.arg(1) as usize];
                }
                _ => {}
            }
        }

        // For jump tables we don't need to update the instruction, but the jump
        // tables themselves.
        for table in jump_tables {
            for index in 0..table.len() {
                table[index] = offsets[table[index] as usize] as u32;
            }
        }

        buffer.write_u32(instructions.len() as u32);

        for ins in instructions {
            buffer.write_u8(ins.opcode.to_int());

            for index in 0..ins.opcode.arity() {
                buffer.write_u16(ins.arg(index as usize));
            }
        }
    }

    fn instruction(
        &mut self,
        method: &Method,
        instruction: &mir::Instruction,
        buffer: &mut Vec<Instruction>,
        jump_tables: &mut Vec<Vec<u32>>,
        locations: &mut Vec<(usize, LocationId)>,
    ) {
        match instruction {
            mir::Instruction::Nil(ins) => {
                let reg = ins.register.0 as u16;

                buffer.push(Instruction::one(Opcode::GetNil, reg));
            }
            mir::Instruction::CheckRefs(ins) => {
                let reg = ins.register.0 as u16;

                buffer.push(Instruction::one(Opcode::CheckRefs, reg));
            }
            mir::Instruction::Reduce(ins) => {
                buffer.push(Instruction::one(Opcode::Reduce, ins.amount));
            }
            mir::Instruction::Goto(ins) => {
                let block_id = ins.block.0 as u16;

                buffer.push(Instruction::one(Opcode::Goto, block_id));
            }
            mir::Instruction::Branch(ins) => {
                let reg = ins.condition.0 as u16;
                let ok = ins.if_true.0 as u16;
                let err = ins.if_false.0 as u16;

                buffer.push(Instruction::three(Opcode::Branch, reg, ok, err));
            }
            mir::Instruction::BranchResult(ins) => {
                let ok = ins.ok.0 as u16;
                let err = ins.error.0 as u16;

                buffer.push(Instruction::two(Opcode::BranchResult, ok, err));
            }
            mir::Instruction::JumpTable(ins) => {
                let reg = ins.register.0 as u16;
                let idx = jump_tables.len() as u16;
                let table = ins.blocks.iter().map(|b| b.0 as u32).collect();

                jump_tables.push(table);
                buffer.push(Instruction::two(Opcode::JumpTable, reg, idx));
            }
            mir::Instruction::Return(ins) => {
                let reg = ins.register.0 as u16;

                buffer.push(Instruction::one(Opcode::Return, reg));
            }
            mir::Instruction::ReturnAsync(ins) => {
                let op = Opcode::ProcessWriteResult;
                let reg = ins.register.0 as u16;
                let val = ins.value.0 as u16;

                buffer.push(Instruction::three(op, reg, val, 0));
            }
            mir::Instruction::Throw(ins) => {
                let reg = ins.register.0 as u16;
                let unwind = 1;

                buffer.push(Instruction::two(Opcode::Throw, reg, unwind));
            }
            mir::Instruction::ThrowAsync(ins) => {
                let op = Opcode::ProcessWriteResult;
                let reg = ins.register.0 as u16;
                let val = ins.value.0 as u16;

                buffer.push(Instruction::three(op, reg, val, 1));
            }
            mir::Instruction::Finish(_) => {
                buffer.push(Instruction::zero(Opcode::ProcessFinishTask));
            }
            mir::Instruction::AllocateArray(ins) => {
                let op = Opcode::ArrayAllocate;
                let reg = ins.register.0 as u16;

                push_values(buffer, &ins.values);
                buffer.push(Instruction::one(op, reg));
            }
            mir::Instruction::True(ins) => {
                let reg = ins.register.0 as u16;

                buffer.push(Instruction::one(Opcode::GetTrue, reg));
            }
            mir::Instruction::False(ins) => {
                let reg = ins.register.0 as u16;

                buffer.push(Instruction::one(Opcode::GetFalse, reg));
            }
            mir::Instruction::Float(ins) => {
                let op = Opcode::GetConstant;
                let reg = ins.register.0 as u16;
                let idx = self.constant_index(Constant::Float(ins.value));
                let (arg1, arg2) = split_u32(idx);

                buffer.push(Instruction::three(op, reg, arg1, arg2));
            }
            mir::Instruction::Int(ins) => {
                let op = Opcode::GetConstant;
                let reg = ins.register.0 as u16;
                let idx = self.constant_index(Constant::Int(ins.value));
                let (arg1, arg2) = split_u32(idx);

                buffer.push(Instruction::three(op, reg, arg1, arg2));
            }
            mir::Instruction::String(ins) => {
                let op = Opcode::GetConstant;
                let reg = ins.register.0 as u16;
                let idx =
                    self.constant_index(Constant::String(ins.value.clone()));
                let (arg1, arg2) = split_u32(idx);

                buffer.push(Instruction::three(op, reg, arg1, arg2));
            }
            mir::Instruction::Strings(ins) => {
                let op = Opcode::StringConcat;
                let reg = ins.register.0 as u16;

                push_values(buffer, &ins.values);
                buffer.push(Instruction::one(op, reg));
            }
            mir::Instruction::MoveRegister(ins) => {
                let reg = ins.target.0 as u16;
                let src = ins.source.0 as u16;

                buffer.push(Instruction::two(Opcode::MoveRegister, reg, src));
            }
            mir::Instruction::MoveResult(ins) => {
                let reg = ins.register.0 as u16;

                buffer.push(Instruction::one(Opcode::MoveResult, reg));
            }
            mir::Instruction::Allocate(ins) => {
                let reg = ins.register.0 as u16;
                let idx = self.class_info.get(&ins.class).unwrap().index;
                let (arg1, arg2) = split_u32(idx);
                let op = if ins.class.kind(self.db).is_async() {
                    Opcode::ProcessAllocate
                } else {
                    Opcode::Allocate
                };

                buffer.push(Instruction::three(op, reg, arg1, arg2));
            }
            mir::Instruction::Call(ins) => {
                let op = Opcode::CallVirtual;
                let rec = ins.receiver.0 as u16;
                let idx = self.method_info.get(&ins.method).unwrap().index;

                buffer.push(Instruction::one(Opcode::Push, rec));
                push_values(buffer, &ins.arguments);
                locations.push((buffer.len(), ins.location));
                buffer.push(Instruction::two(op, rec, idx));
            }
            mir::Instruction::CallBuiltin(ins) => {
                let op = Opcode::BuiltinFunctionCall;
                let idx = ins.id.to_int();

                push_values(buffer, &ins.arguments);
                locations.push((buffer.len(), ins.location));
                buffer.push(Instruction::one(op, idx));
            }
            mir::Instruction::CallClosure(ins) => {
                let op = Opcode::CallVirtual;
                let rec = ins.receiver.0 as u16;
                let idx = CALL_CLOSURE_INDEX;

                buffer.push(Instruction::one(Opcode::Push, rec));
                push_values(buffer, &ins.arguments);
                locations.push((buffer.len(), ins.location));
                buffer.push(Instruction::two(op, rec, idx));
            }
            mir::Instruction::CallDropper(ins) => {
                let op = Opcode::CallVirtual;
                let rec = ins.receiver.0 as u16;
                let idx = DROPPER_INDEX;

                buffer.push(Instruction::one(Opcode::Push, rec));
                locations.push((buffer.len(), ins.location));
                buffer.push(Instruction::two(op, rec, idx));
            }
            mir::Instruction::CallDynamic(ins) => {
                let op = Opcode::CallDynamic;
                let rec = ins.receiver.0 as u16;
                let (hash1, hash2) =
                    split_u32(self.method_info.get(&ins.method).unwrap().hash);

                buffer.push(Instruction::one(Opcode::Push, rec));
                push_values(buffer, &ins.arguments);
                locations.push((buffer.len(), ins.location));
                buffer.push(Instruction::three(op, rec, hash1, hash2));
            }
            mir::Instruction::Clone(ins) => {
                let reg = ins.register.0 as u16;
                let src = ins.source.0 as u16;
                let op = match ins.kind {
                    CloneKind::Float => Opcode::FloatClone,
                    CloneKind::Int => Opcode::IntClone,
                    CloneKind::Process | CloneKind::String => Opcode::Increment,
                    CloneKind::Other => Opcode::MoveRegister,
                };

                buffer.push(Instruction::two(op, reg, src));
            }
            mir::Instruction::Decrement(ins) => {
                let op = Opcode::Decrement;
                let reg = ins.register.0 as u16;

                locations.push((buffer.len(), ins.location));
                buffer.push(Instruction::one(op, reg));
            }
            mir::Instruction::DecrementAtomic(ins) => {
                let op = Opcode::DecrementAtomic;
                let reg = ins.register.0 as u16;
                let val = ins.value.0 as u16;

                locations.push((buffer.len(), ins.location));
                buffer.push(Instruction::two(op, reg, val));
            }
            mir::Instruction::Free(ins) => {
                let op = Opcode::Free;
                let reg = ins.register.0 as u16;

                buffer.push(Instruction::one(op, reg));
            }
            mir::Instruction::GetClass(ins) => {
                let op = Opcode::GetClass;
                let reg = ins.register.0 as u16;
                let idx = self.class_info.get(&ins.id).unwrap().index;
                let (arg1, arg2) = split_u32(idx);

                buffer.push(Instruction::three(op, reg, arg1, arg2));
            }
            mir::Instruction::GetConstant(ins) => {
                let op = Opcode::GetConstant;
                let reg = ins.register.0 as u16;
                let val = self.mir.constants.get(&ins.id).cloned().unwrap();
                let (idx1, idx2) = split_u32(self.constant_index(val));

                buffer.push(Instruction::three(op, reg, idx1, idx2));
            }
            mir::Instruction::GetField(ins) => {
                let rec_typ = method.registers.value_type(ins.receiver);
                let stype = method.id.self_type(self.db);
                let op = if rec_typ.is_async(self.db, stype) {
                    Opcode::ProcessGetField
                } else {
                    Opcode::GetField
                };

                let reg = ins.register.0 as u16;
                let rec = ins.receiver.0 as u16;
                let idx = ins.field.index(self.db) as u16;

                buffer.push(Instruction::three(op, reg, rec, idx));
            }
            mir::Instruction::SetField(ins) => {
                let rec_typ = method.registers.value_type(ins.receiver);
                let stype = method.id.self_type(self.db);
                let op = if rec_typ.is_async(self.db, stype) {
                    Opcode::ProcessSetField
                } else {
                    Opcode::SetField
                };

                let rec = ins.receiver.0 as u16;
                let idx = ins.field.index(self.db) as u16;
                let val = ins.value.0 as u16;

                buffer.push(Instruction::three(op, rec, idx, val));
            }
            mir::Instruction::GetModule(ins) => {
                let op = Opcode::GetModule;
                let reg = ins.register.0 as u16;
                let (idx1, idx2) = split_u32(ins.id.0);

                buffer.push(Instruction::three(op, reg, idx1, idx2));
            }
            mir::Instruction::Increment(ins) => {
                let op = Opcode::Increment;
                let reg = ins.register.0 as u16;
                let val = ins.value.0 as u16;

                buffer.push(Instruction::two(op, reg, val));
            }
            mir::Instruction::IntEq(ins) => {
                let op = Opcode::IntEq;
                let reg = ins.register.0 as u16;
                let left = ins.left.0 as u16;
                let right = ins.right.0 as u16;

                buffer.push(Instruction::three(op, reg, left, right));
            }
            mir::Instruction::StringEq(ins) => {
                let op = Opcode::StringEq;
                let reg = ins.register.0 as u16;
                let left = ins.left.0 as u16;
                let right = ins.right.0 as u16;

                buffer.push(Instruction::three(op, reg, left, right));
            }
            mir::Instruction::RawInstruction(ins) => {
                let mut args = [0, 0, 0, 0, 0];

                for (idx, arg) in args.iter_mut().enumerate() {
                    if let Some(reg) = ins.arguments.get(idx) {
                        *arg = reg.0 as u16;
                    } else {
                        break;
                    }
                }

                locations.push((buffer.len(), ins.location));
                buffer.push(Instruction::new(ins.opcode, args));
            }
            mir::Instruction::RefKind(ins) => {
                let op = Opcode::RefKind;
                let reg = ins.register.0 as u16;
                let val = ins.value.0 as u16;

                buffer.push(Instruction::two(op, reg, val));
            }
            mir::Instruction::Send(ins) => {
                let op = Opcode::ProcessSend;
                let rec = ins.receiver.0 as u16;
                let idx = self.method_info.get(&ins.method).unwrap().index;
                let wait = ins.wait as u16;

                buffer.push(Instruction::one(Opcode::Push, rec));
                push_values(buffer, &ins.arguments);
                locations.push((buffer.len(), ins.location));
                buffer.push(Instruction::three(op, rec, idx, wait));
            }
            mir::Instruction::SendAsync(ins) => {
                let op = Opcode::ProcessSendAsync;
                let reg = ins.register.0 as u16;
                let rec = ins.receiver.0 as u16;
                let idx = self.method_info.get(&ins.method).unwrap().index;

                buffer.push(Instruction::one(Opcode::Push, rec));
                push_values(buffer, &ins.arguments);
                locations.push((buffer.len(), ins.location));
                buffer.push(Instruction::three(op, reg, rec, idx));
            }
            mir::Instruction::Drop(_) => {
                // This instruction is expanded before converting MIR to
                // bytecode, and thus shouldn't occur at this point.
                unreachable!();
            }
        }
    }

    fn location_table(
        &mut self,
        buffer: &mut Buffer,
        locations: Vec<(usize, LocationId)>,
    ) {
        let mut entries = Vec::new();
        let mut offset = 0_u16;

        for (index, id) in locations {
            offset = index as u16 - offset;

            let loc = self.mir.location(id);
            let name = loc.method.name(self.db).clone();
            let file = loc.module.file(self.db).to_string_lossy().into_owned();
            let line = *loc.range.line_range.start() as u16;
            let file_idx = self.constant_index(Constant::String(file));
            let name_idx = self.constant_index(Constant::String(name));

            entries.push((offset, line, file_idx, name_idx));
        }

        buffer.write_u16(entries.len() as u16);

        for (offset, line, file, name) in entries {
            buffer.write_u16(offset);
            buffer.write_u16(line);
            buffer.write_u32(file);
            buffer.write_u32(name);
        }
    }

    fn constant_index(&mut self, constant: Constant) -> u32 {
        let len = self.constant_indexes.len() as u32;

        *self.constant_indexes.entry(constant).or_insert(len)
    }
}
