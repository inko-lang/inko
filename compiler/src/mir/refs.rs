//! MIR passes for optimizing reference and borrow counting.
use crate::mir::{Goto, Instruction, Method, MoveRegister, RegisterId};
use types::Database;

#[derive(Copy, Clone, Debug)]
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

    fn len(&self) -> usize {
        self.map.len()
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

/// A pass that optimizes reference counts for strings.
pub(crate) struct OptimizeStrings<'a> {
    db: &'a Database,
    method: &'a mut Method,
}

impl<'a> OptimizeStrings<'a> {
    pub(crate) fn new(db: &'a Database, method: &'a mut Method) -> Self {
        Self { db, method }
    }

    pub(crate) fn run(self) {
        let mut values = Values::new(self.method.registers.len());

        for &reg in &self.method.arguments {
            if self.method.registers.value_type(reg).is_string(self.db) {
                values.add_runtime(reg);
            }
        }

        for block in &self.method.body.blocks {
            for ins in &block.instructions {
                let (cons, reg) = match ins {
                    Instruction::String(i) => (true, i.register),
                    Instruction::GetConstant(i) => (true, i.register),
                    Instruction::Cast(i) => (false, i.register),
                    Instruction::GetField(i) => (false, i.register),
                    Instruction::ReadPointer(i) => (false, i.register),
                    Instruction::CallBuiltin(i) => (false, i.register),
                    Instruction::CallInstance(i) => (false, i.register),
                    Instruction::CallStatic(i) => (false, i.register),
                    Instruction::CallDynamic(i) => (false, i.register),
                    Instruction::CallClosure(i) => (false, i.register),
                    _ => continue,
                };

                if !self.method.registers.value_type(reg).is_string(self.db) {
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
        let mut merged = vec![false; self.method.registers.len()];
        let mut run = true;

        // It's possible the graph is constructed such that we can't propagate
        // the values across registers in a single iteration, regardless of what
        // order we iterate in. As such we keep iterating until we run out of
        // registers to update.
        //
        // Testing using a few Inko applications (e.g. shost) shows that in most
        // cases no more than 3-5 iterations are necessary, with a small amount
        // of cases requiring more iterations.
        while run {
            run = false;

            for block in &self.method.body.blocks {
                for ins in &block.instructions {
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

        let mut keep_all = vec![false; self.method.registers.len()];
        let mut keep_dec = vec![false; self.method.registers.len()];
        let mut assigned = vec![0_u8; self.method.registers.len()];

        for block in &self.method.body.blocks {
            for ins in &block.instructions {
                match ins {
                    // If the source is an expression (other than a local
                    // variable reference) assigned to a variable, we need to
                    // keep the decrements for when that variable goes out of
                    // scope.
                    //
                    // However, borrowing the variable doesn't mandate an
                    // increment _unless_ the borrow escapes.
                    //
                    // TODO: we need to reliably detect the difference between
                    // "a move to define a variable" and "a move that just
                    // moves" (e.g. a branch).
                    Instruction::MoveRegister(i) => {
                        let is_var =
                            self.method.registers.is_variable(i.target);

                        if is_var && assigned[i.target.0] < 2 {
                            assigned[i.target.0] += 1;
                        }

                        if assigned[i.target.0] > 1 {
                            keep_all[i.target.0] = true;
                            continue;
                        }

                        if is_var && keep_all[i.source.0] {
                            keep_dec[i.target.0] = true;
                        }
                    }
                    Instruction::CallBuiltin(i) => {
                        i.arguments.iter().for_each(|r| keep_all[r.0] = true);
                        keep_all[i.register.0] = true;
                    }
                    Instruction::CallDynamic(i) => {
                        i.arguments.iter().for_each(|r| keep_all[r.0] = true);
                        keep_all[i.register.0] = true;
                    }
                    Instruction::CallStatic(i) => {
                        i.arguments.iter().for_each(|r| keep_all[r.0] = true);
                        keep_all[i.register.0] = true;
                    }
                    Instruction::CallInstance(i) => {
                        if i.method.is_moving(self.db) {
                            keep_all[i.receiver.0] = true;
                        }

                        i.arguments.iter().for_each(|r| keep_all[r.0] = true);
                        keep_all[i.register.0] = true;
                    }
                    Instruction::CallExtern(i) => {
                        i.arguments.iter().for_each(|r| keep_all[r.0] = true);
                        keep_all[i.register.0] = true;
                    }
                    Instruction::CallClosure(i) => {
                        i.arguments.iter().for_each(|r| keep_all[r.0] = true);
                        keep_all[i.register.0] = true;
                    }
                    Instruction::Send(i) => {
                        i.arguments.iter().for_each(|r| keep_all[r.0] = true);
                    }
                    Instruction::SetField(i) => keep_all[i.value.0] = true,
                    Instruction::WritePointer(i) => keep_all[i.value.0] = true,
                    Instruction::ReadPointer(i) => {
                        keep_all[i.register.0] = true
                    }
                    Instruction::Return(i) => keep_all[i.register.0] = true,
                    Instruction::GetField(i) => keep_all[i.register.0] = true,
                    Instruction::Cast(i) => keep_all[i.register.0] = true,
                    _ => {}
                }
            }
        }

        for (i, &keep) in keep_all.iter().enumerate() {
            if keep {
                keep_dec[i] = true;
            }
        }

        let mut reconnect = false;

        for block in &mut self.method.body.blocks {
            for ins in &mut block.instructions {
                match ins {
                    Instruction::IncrementAtomic(i)
                        if values.get(i.source).is_constant() =>
                    {
                        *ins =
                            Instruction::MoveRegister(Box::new(MoveRegister {
                                source: i.source,
                                target: i.target,
                                location: i.location,
                                volatile: false,
                            }));
                    }
                    Instruction::DecrementAtomic(i)
                        if values.get(i.register).is_constant() =>
                    {
                        let after_blk = i.if_false;
                        let loc = i.location;

                        reconnect = true;
                        *ins = Instruction::Goto(Box::new(Goto {
                            block: after_blk,
                            location: loc,
                        }));
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

        // TODO: not memory efficient
        // TODO: this causes shost to respond with 404s, so we probably still
        // drop strings prematurely

        let mut remove_incr = Vec::new();
        let mut remove_decr = Vec::new();

        // TODO: at the MIR level we can't detect if variable A was assigned
        // variable B or some _expression_ B.
        //
        // I think we need the "this is a variable register" flag for this.
        //
        // Akshually: given `let a = b` if `b` is an expression that returns a
        // string, the IR is `a = move b`. If on the other hand it's e.g. a
        // variable it will be `a = increment_atomic b`. Is that reliably
        // though? We may still end up eliminating unrelated decrements.
        //
        // More precisely, given register R I think we can remove _all_
        // increments and decrements while keeping decrements when:
        //
        // 1. R stores a method argument
        // 2. R is assigned the result of a non-local variable expression
        // 3. R escapes
        for bid in self.method.body.iter() {
            for (iid, ins) in
                self.method.body.block(bid).instructions.iter().enumerate()
            {
                match ins {
                    Instruction::IncrementAtomic(i)
                        if !keep_all[i.source.0] && !keep_all[i.target.0] =>
                    {
                        // TODO: don't clone the entire instruction
                        // TODO: determine if we need a stack or just a single
                        // slot
                        reconnect = true;
                        remove_incr.push((bid, iid, (*i).clone()));
                    }
                    Instruction::DecrementAtomic(i)
                        if !keep_dec[i.register.0] =>
                    {
                        reconnect = true;
                        remove_decr.push((bid, iid, (*i).clone()));
                    }
                    _ => continue,
                }
            }
        }

        for (bid, iid, ins) in remove_incr {
            self.method.body.block_mut(bid).instructions[iid] =
                Instruction::MoveRegister(Box::new(MoveRegister {
                    source: ins.source,
                    target: ins.target,
                    volatile: false,
                    location: ins.location,
                }));
        }

        for (bid, iid, ins) in remove_decr {
            self.method.body.block_mut(bid).instructions[iid] =
                Instruction::Goto(Box::new(Goto {
                    block: ins.if_false,
                    location: ins.location,
                }));
        }

        if reconnect {
            self.method.reconnect_blocks();
            self.method.remove_unreachable_blocks();
        }
    }
}
