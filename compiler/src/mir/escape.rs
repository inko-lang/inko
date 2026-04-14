use crate::graph;
use crate::mir::{Instruction, Method, RegisterId, SELF_ID};
use indexmap::IndexSet;
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::mem::swap;
use types::format::format_type;
use types::module_name::ModuleName;
use types::{Database, MethodId, TypeRef};

#[derive(Copy, Clone, Debug)]
enum Escape {
    Yes,
    No,
    Inner,
}

impl Escape {
    fn is_escape(self) -> bool {
        matches!(self, Escape::Yes)
    }

    fn no_escape(self) -> bool {
        matches!(self, Escape::No)
    }

    fn is_inner_escape(self) -> bool {
        matches!(self, Escape::Inner)
    }
}

/// Escape information for a single MIR method.
#[derive(Clone)]
pub(crate) struct Entry {
    /// Each argument and their escape status, `true` meaning the argument
    /// escapes the method.
    ///
    /// This list doesn't include the self argument/register that's part of the
    /// MIR method arguments list.
    arguments: Vec<Escape>,

    /// Indicates if the receiver escapes the method or not.
    receiver: Escape,
}

impl Entry {
    fn new() -> Self {
        Self { arguments: Vec::new(), receiver: Escape::Yes }
    }

    fn argument(&self, index: usize) -> Escape {
        self.arguments.get(index).cloned().unwrap_or(Escape::Yes)
    }
}

/// Escape information for each MIR method.
pub(crate) struct Entries {
    map: HashMap<MethodId, Entry>,
}

impl Entries {
    pub(crate) fn new() -> Self {
        Self { map: HashMap::new() }
    }

    fn get(&self, id: MethodId) -> Option<&Entry> {
        self.map.get(&id)
    }

    fn insert(&mut self, id: MethodId, entry: Entry) {
        self.map.insert(id, entry);
    }
}

/// A node in the escape graph.
struct Node {
    register: RegisterId,
    escape: Escape,
}

impl Node {
    fn new(register: RegisterId) -> Self {
        Self { register, escape: Escape::No }
    }
}

struct State {
    /// All the nodes in the escape graph.
    graph: graph::Graph<Node>,

    /// A mapping of register IDs to their graph nodes.
    map: HashMap<RegisterId, graph::NodeId>,

    /// The registers to process while traversing a register graph.
    work: Vec<RegisterId>,

    /// The registers processed while traversing a register graph.
    done: HashSet<RegisterId>,
}

impl State {
    fn new() -> Self {
        Self {
            graph: graph::Graph::new(),
            map: HashMap::new(),
            work: Vec::new(),
            done: HashSet::new(),
        }
    }

    fn get_or_add(&mut self, register: RegisterId) -> graph::NodeId {
        *self
            .map
            .entry(register)
            .or_insert_with(|| self.graph.add(Node::new(register)))
    }

    fn add_edge(&mut self, from: RegisterId, to: RegisterId) {
        let from = self.get_or_add(from);
        let to = self.get_or_add(to);

        self.graph.add_edge(from, to);
    }

    fn escapes(&mut self, register: RegisterId) {
        let node = self.get_or_add(register);

        self.graph.get_mut(node).value.escape = Escape::Yes;
    }

    fn inner_escapes(&mut self, register: RegisterId) {
        let node = self.get_or_add(register);

        self.graph.get_mut(node).value.escape = Escape::Inner;
        self.outgoing_escape(node);
    }

    fn escape_state(&mut self, register: RegisterId) -> Escape {
        let mut state = Escape::No;

        self.work.push(register);
        self.done.insert(register);

        while let Some(register) = self.work.pop() {
            // If there's no node it means the register wasn't seen anywhere,
            // which in turn means it was never moved.
            let Some(&id) = self.map.get(&register) else {
                break;
            };

            let node = self.graph.get(id);

            match node.value.escape {
                Escape::Yes => {
                    state = Escape::Yes;
                    break;
                }
                // We _don't_ short-circuit in this case because if a dependency
                // escapes fully it should take precedence.
                Escape::Inner => state = Escape::Inner,
                _ => {}
            }

            for &id in &node.incoming {
                let reg = self.graph.get(id).value.register;

                if self.done.insert(reg) {
                    self.work.push(reg);
                }
            }
        }

        self.work.clear();
        self.done.clear();
        state
    }

    fn outgoing_escape(&mut self, id: graph::NodeId) {
        for &id in &self.graph.get(id).outgoing {
            self.work.push(self.graph.get(id).value.register);
        }

        while let Some(r) = self.work.pop() {
            self.escapes(r);
        }
    }
}

#[derive(Eq, PartialEq)]
enum AllocationKind {
    Heap,
    Stack,
}

#[derive(Eq, PartialEq)]
struct Allocation {
    line: u32,
    column: u32,
    value_type: TypeRef,
    kind: AllocationKind,
}

impl PartialOrd for Allocation {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Allocation {
    fn cmp(&self, other: &Self) -> Ordering {
        match self.line.cmp(&other.line) {
            Ordering::Equal => self.column.cmp(&other.column),
            ord => ord,
        }
    }
}

pub(crate) struct Stats {
    escaping: u64,
    promoted: u64,
    allocations: Vec<Allocation>,
}

impl Stats {
    pub(crate) fn new() -> Self {
        Self { escaping: 0, promoted: 0, allocations: Vec::new() }
    }

    pub(crate) fn merge(&mut self, mut other: Stats) {
        self.escaping += other.escaping;
        self.promoted += other.promoted;
        self.allocations.append(&mut other.allocations);
    }

    pub(crate) fn show_statistics(&self) {
        let total = self.promoted + self.escaping;
        let perc_promoted = (self.promoted as f64 / total as f64) * 100.0;
        let perc_escaping = (self.escaping as f64 / total as f64) * 100.0;

        println!(
            "Escape analysis:
  Promoted  {} ({:.0}%)
  Escaping  {} ({:.0}%)
  Total     {}",
            self.promoted, perc_promoted, self.escaping, perc_escaping, total
        );
    }

    pub(crate) fn show_allocations(&mut self, db: &Database) {
        self.allocations.sort();

        for alloc in &self.allocations {
            let name = format_type(db, alloc.value_type);
            let kind = match alloc.kind {
                AllocationKind::Stack => "stack",
                AllocationKind::Heap => "heap",
            };

            println!("{}:{} {} {}", alloc.line, alloc.column, name, kind);
        }
    }
}

/// A pass that runs escape analysis on a single MIR method.
pub(crate) struct AnalyzeMethod<'a> {
    db: &'a Database,
    entries: &'a mut Entries,
    method: &'a mut Method,
    state: State,

    /// The registers containing values _moved_ out of fields.
    fields: IndexSet<RegisterId>,
}

impl<'a> AnalyzeMethod<'a> {
    pub(crate) fn new(
        db: &'a Database,
        state: &'a mut Entries,
        method: &'a mut Method,
    ) -> Self {
        Self {
            db,
            entries: state,
            state: State::new(),
            method,
            fields: IndexSet::new(),
        }
    }

    pub(crate) fn run(
        mut self,
        show_escapes_for: Option<&ModuleName>,
    ) -> Stats {
        self.connect_nodes();
        self.mark_escaping();

        let mut stats = self.promote_allocations();

        self.add_entry();

        if let Some(path) = show_escapes_for {
            self.collect_allocations(&mut stats, path);
        }

        stats
    }

    fn connect_nodes(&mut self) {
        self.method.body.each_block_in_order(|bid| {
            for ins in &self.method.body.blocks[bid.0].instructions {
                let (from, to) = match ins {
                    Instruction::MoveRegister(i) => (i.target, i.source),
                    Instruction::GetField(i)
                        if self
                            .method
                            .registers
                            .value_type(i.register)
                            .is_owned_or_uni(self.db) =>
                    {
                        self.fields.insert(i.register);
                        (i.register, i.receiver)
                    }
                    Instruction::SetField(i)
                        if self
                            .method
                            .registers
                            .value_type(i.value)
                            .is_owned_or_uni(self.db) =>
                    {
                        (i.receiver, i.value)
                    }
                    _ => continue,
                };

                self.state.add_edge(from, to);
            }
        });
    }

    fn mark_escaping(&mut self) {
        for block in &self.method.body.blocks {
            for ins in &block.instructions {
                match ins {
                    Instruction::CallDynamic(i) => {
                        for &r in &i.arguments {
                            if self.is_escape_candidate(r) {
                                self.state.escapes(r);
                            }
                        }
                    }
                    Instruction::CallStatic(i) => {
                        let callee = if let Some(v) = self.entries.get(i.method)
                        {
                            v
                        } else {
                            &Entry::new()
                        };

                        for (idx, &reg) in i.arguments.iter().enumerate() {
                            match callee.argument(idx) {
                                Escape::No => {}
                                Escape::Inner => self.state.inner_escapes(reg),
                                Escape::Yes => self.state.escapes(reg),
                            }
                        }
                    }
                    Instruction::CallInstance(i) => {
                        let rec = i.receiver;
                        let met = i.method;
                        let callee = if let Some(v) = self.entries.get(met) {
                            v
                        } else {
                            &Entry::new()
                        };

                        if met.is_moving(self.db) && callee.receiver.is_escape()
                        {
                            self.state.escapes(rec);
                        }

                        // Mutating methods act on mutable _borrows_ but may
                        // still swap field values and cause the old values to
                        // escape.
                        if met.is_mutable(self.db)
                            && callee.receiver.is_inner_escape()
                        {
                            self.state.inner_escapes(rec);
                        }

                        for (idx, &reg) in i.arguments.iter().enumerate() {
                            match callee.argument(idx) {
                                Escape::No => {}
                                Escape::Inner => self.state.inner_escapes(reg),
                                Escape::Yes => self.state.escapes(reg),
                            }
                        }
                    }
                    Instruction::CallExtern(i) => {
                        for &r in &i.arguments {
                            if self.is_escape_candidate(r) {
                                self.state.escapes(r);
                            }
                        }
                    }
                    // Similar to dynamic dispatch, when calling an opaque
                    // closure we have no idea what it might do with its
                    // arguments and thus have to assume they escape.
                    Instruction::CallClosure(i) => {
                        for &r in &i.arguments {
                            if self.is_escape_candidate(r) {
                                self.state.escapes(r);
                            }
                        }
                    }
                    // Data sent across a process boundary has an unclear
                    // lifetime and thus is always considered to be escaping.
                    Instruction::Send(i) => {
                        for &r in &i.arguments {
                            if self.is_escape_candidate(r) {
                                self.state.escapes(r);
                            }
                        }
                    }
                    // Borrows may be introduced in many ways and it's difficult
                    // to accurately track whether a value escapes through a
                    // borrow or not, so we conservatively assume that it will.
                    Instruction::SetField(i)
                        if self
                            .method
                            .registers
                            .value_type(i.receiver)
                            .is_mut(self.db)
                            && self.is_escape_candidate(i.value) =>
                    {
                        self.state.escapes(i.value);
                    }
                    Instruction::SetField(i)
                        if i.type_id.is_async(self.db)
                            && self.is_escape_candidate(i.value) =>
                    {
                        self.state.escapes(i.value);
                    }
                    // We don't have the ability to reliably track if data
                    // written to a pointer outlives the allocation frame or
                    // not, so we assume such data escapes.
                    Instruction::WritePointer(i)
                        if self.is_escape_candidate(i.value) =>
                    {
                        self.state.escapes(i.value);
                    }
                    Instruction::Cast(i)
                        if self.is_escape_candidate(i.source) =>
                    {
                        self.state.escapes(i.source);
                    }
                    Instruction::Return(i)
                        if self.is_escape_candidate(i.register) =>
                    {
                        self.state.escapes(i.register);
                    }
                    _ => {}
                }
            }
        }
    }

    fn promote_allocations(&mut self) -> Stats {
        let mut stats = Stats::new();

        for block in &mut self.method.body.blocks {
            for ins in &mut block.instructions {
                let Instruction::Allocate(i) = ins else { continue };

                if !i.type_id.is_regular_heap_value(self.db) {
                    continue;
                }

                if self.state.escape_state(i.register).is_escape() {
                    stats.escaping += 1;
                } else {
                    i.stack = true;
                    stats.promoted += 1;
                }
            }
        }

        // We don't remove/modify Free instructions here because:
        //
        // 1. There's no need because they already perform a "is this a heap or
        //    stack value?" check
        // 2. In many instances the same Free instruction may operate on both
        //    stack and heap allocated values, such that we need to keep the
        //    instruction anyway

        stats
    }

    fn add_entry(&mut self) {
        let mut entry = Entry::new();

        for &reg in &self.method.arguments {
            // The MIR arguments include `self` but we don't want that for our
            // escape analysis.
            if reg.0 == SELF_ID && self.method.id.is_instance(self.db) {
                continue;
            }

            entry.arguments.push(if self.is_escape_candidate(reg) {
                self.state.escape_state(reg)
            } else {
                Escape::No
            });
        }

        let self_reg = RegisterId(SELF_ID);

        entry.receiver = if self.method.id.is_moving(self.db)
            && self.is_escape_candidate(self_reg)
        {
            self.state.escape_state(self_reg)
        } else {
            Escape::No
        };

        let mut fields = IndexSet::new();

        // We can't iterate over `self.fields` below while also using other
        // methods in `self`, so we perform this swap to work around that.
        swap(&mut fields, &mut self.fields);

        for reg in fields {
            if !self.is_escape_candidate(reg) {
                continue;
            }

            if self.state.escape_state(reg).is_escape()
                && entry.receiver.no_escape()
            {
                entry.receiver = Escape::Inner;
            }
        }

        self.entries.insert(self.method.id, entry);
    }

    fn collect_allocations(&self, stats: &mut Stats, module: &ModuleName) {
        for block in &self.method.body.blocks {
            for ins in &block.instructions {
                let Instruction::Allocate(ins) = ins else { continue };

                if !ins.type_id.is_regular_heap_value(self.db) {
                    continue;
                }

                let src_method = ins
                    .location
                    .inlined_call_id()
                    .map(|i| self.method.inlined_calls[i].source_method)
                    .unwrap_or(self.method.id);

                if src_method.source_module(self.db).method_symbol_name(self.db)
                    != module
                {
                    continue;
                }

                let typ = self.method.registers.value_type(ins.register);

                stats.allocations.push(Allocation {
                    line: ins.location.line,
                    column: ins.location.column,
                    kind: if ins.stack {
                        AllocationKind::Stack
                    } else {
                        AllocationKind::Heap
                    },
                    value_type: typ,
                });
            }
        }
    }

    fn is_escape_candidate(&self, register: RegisterId) -> bool {
        self.method.registers.value_type(register).is_owned_or_uni(self.db)
    }
}
