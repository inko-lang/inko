use crate::mir::{
    BlockId, Goto, Graph, InlinedCall, InlinedCalls, Instruction,
    InstructionLocation, Method, Mir, MoveRegister, RegisterId, Registers,
};
use crate::state::State;
use std::cmp::min;
use std::collections::HashSet;
use types::{Database, Inline, MethodId, ModuleId};

/// If a method wouldn't be inlined but is called at most this many times, it
/// will still be inlined.
///
/// The goal of this setting is to allow inlining of methods that aren't used
/// much, such as (large) private helper methods.
const INLINE_ANYWAY_CALL_COUNT: u16 = 2;

/// The maximum weight a method is allowed to have before we stop inlining other
/// methods into it.
///
/// The current threshold is rather conservative in order to reduce the amount
/// of LLVM IR we produce, as more LLVM IR results in (drastically) more
/// compile-time memory usage.
const MAX_WEIGHT: u16 = 100;

fn instruction_weight(db: &Database, instruction: &Instruction) -> u16 {
    // The weights are mostly arbitrary and are meant to be a rough resemblance
    // of the final code size. We don't count all instructions as many translate
    // into a single (trivial) machine instruction. Instead, we count the
    // instructions that are expected to produce more code, branches, etc.
    match instruction {
        // Stack allocations don't translate into machine instructions, so we
        // give them a weight of zero. Regular allocations and spawning
        // processes translate into a function call, so we give them the same
        // weight as calls.
        Instruction::Allocate(ins) if ins.type_id.is_stack_allocated(db) => 0,
        Instruction::Allocate(_) => 1,
        Instruction::Spawn(_) => 1,

        // These instructions translate into (more or less) regular function
        // calls, which don't take up that much space.
        Instruction::Free(_) => 1,
        Instruction::CallInstance(_) => 1,
        Instruction::CallStatic(_) => 1,
        Instruction::CallClosure(_) => 1,
        Instruction::CallDropper(_) => 1,

        // These instructions introduce one or more branches, though the
        // instructions themselves are pretty small.
        Instruction::CheckRefs(_) => 1,
        Instruction::DecrementAtomic(_) => 1,
        Instruction::Switch(_) => 1,

        // Branches may introduce many basic blocks and code, especially when
        // used for nested if-else trees (such as when matching against an
        // integer). As such, we give it a greater weight than Switch.
        Instruction::Branch(_) => 2,

        // These instructions translate into a bit more code (potentially), such
        // as dynamic dispatch when probing is necessary.
        Instruction::Send(_) => 2,
        Instruction::CallDynamic(_) => 2,
        _ => 0,
    }
}

pub(crate) fn method_weight(db: &Database, method: &Method) -> u16 {
    let mut weight = 0_u16;

    for block in &method.body.blocks {
        for ins in &block.instructions {
            weight = weight.saturating_add(instruction_weight(db, ins));
        }
    }

    weight
}

struct Callee {
    registers: Registers,
    body: Graph,
    arguments: Vec<RegisterId>,
    inlined_calls: Vec<InlinedCalls>,
}

struct CallSite {
    /// The register to store the return value in.
    target: RegisterId,

    /// The basic block in which the call instruction resides.
    block: BlockId,

    /// The index of the call instruction.
    instruction: usize,

    /// The ID of the callee.
    id: MethodId,

    /// The registers containing the arguments of the caller.
    ///
    /// For calls to instance methods, the receiver is passed as the first
    /// argument, such that the number of caller and callee arguments matches.
    arguments: Vec<RegisterId>,

    /// The source location of the call.
    location: InstructionLocation,
}

impl CallSite {
    fn new(
        target: RegisterId,
        block: BlockId,
        instruction: usize,
        receiver: Option<RegisterId>,
        arguments: &[RegisterId],
        callee: &Method,
        location: InstructionLocation,
    ) -> CallSite {
        let mut caller_args =
            if let Some(rec) = receiver { vec![rec] } else { Vec::new() };

        caller_args.extend(arguments);
        CallSite {
            target,
            block,
            instruction,
            id: callee.id,
            arguments: caller_args,
            location,
        }
    }
}

impl CallSite {
    fn inline_into(
        self,
        caller: &mut Method,
        mut callee: Callee,
        after_call: BlockId,
    ) {
        let loc = self.location;
        let reg_start = caller.registers.len();
        let blk_start = caller.body.blocks.len();
        let start_id = callee.body.start_id;

        for reg in &mut callee.arguments {
            *reg += reg_start;
        }

        caller.registers.merge(callee.registers);
        caller.body.merge(callee.body);

        // For inlined instructions we need to maintain the inline call stack so
        // we can produce correct debug information.
        let inline_offset = caller.inlined_calls.len() as u32;
        let chain = if let Some(id) = self.location.inlined_call_id() {
            // If the instruction originates from an inlined call, we need to
            // add the source method as the caller instead of `caller.id`,
            // because the latter refers to the method we're inlining into.
            let calls = &caller.inlined_calls[id];
            let mut chain = vec![InlinedCall::new(calls.source_method, loc)];

            chain.extend(calls.chain.clone());
            chain
        } else {
            vec![InlinedCall::new(caller.id, loc)]
        };

        caller.inlined_calls.push(InlinedCalls::new(self.id, chain.clone()));

        for calls in &mut callee.inlined_calls {
            calls.chain.append(&mut chain.clone());
        }

        caller.inlined_calls.append(&mut callee.inlined_calls);

        // Now that the registers and blocks have been added to the caller, we
        // need to update the references accordingly. Since both are stored as a
        // Vec, we just need to "shift" the IDs to the right.
        for blk_idx in blk_start..caller.body.blocks.len() {
            let block = &mut caller.body.blocks[blk_idx];

            for id in
                block.predecessors.iter_mut().chain(block.successors.iter_mut())
            {
                *id += blk_start;
            }

            let mut add_goto = None;

            for ins in &mut block.instructions {
                match ins {
                    Instruction::Branch(ins) => {
                        ins.location.set_inlined_call_id(inline_offset);
                        ins.condition += reg_start;
                        ins.if_true += blk_start;
                        ins.if_false += blk_start;
                    }
                    Instruction::Switch(ins) => {
                        ins.location.set_inlined_call_id(inline_offset);
                        ins.register += reg_start;
                        ins.blocks.iter_mut().for_each(|b| *b += blk_start);
                    }
                    Instruction::Bool(ins) => {
                        ins.location.set_inlined_call_id(inline_offset);
                        ins.register += reg_start;
                    }
                    Instruction::Float(ins) => {
                        ins.location.set_inlined_call_id(inline_offset);
                        ins.register += reg_start;
                    }
                    Instruction::Goto(ins) => {
                        ins.location.set_inlined_call_id(inline_offset);
                        ins.block += blk_start;
                    }
                    Instruction::Int(ins) => {
                        ins.location.set_inlined_call_id(inline_offset);
                        ins.register += reg_start;
                    }
                    Instruction::MoveRegister(ins) => {
                        ins.location.set_inlined_call_id(inline_offset);
                        ins.source += reg_start;
                        ins.target += reg_start;
                    }
                    Instruction::Nil(ins) => {
                        ins.location.set_inlined_call_id(inline_offset);
                        ins.register += reg_start;
                    }
                    Instruction::Return(ret) => {
                        ret.location.set_inlined_call_id(inline_offset);

                        let reg = ret.register + reg_start;
                        let loc = ret.location;

                        *ins =
                            Instruction::MoveRegister(Box::new(MoveRegister {
                                source: reg,
                                target: self.target,
                                volatile: false,
                                location: loc,
                            }));

                        // Return is a terminal instruction and is thus the last
                        // instruction in the block. This means this option
                        // should never be set multiple times.
                        debug_assert!(add_goto.is_none());
                        add_goto = Some(loc);
                    }
                    Instruction::String(ins) => {
                        ins.location.set_inlined_call_id(inline_offset);
                        ins.register += reg_start;
                    }
                    Instruction::CallStatic(ins) => {
                        ins.location.set_inlined_call_id(inline_offset);
                        ins.register += reg_start;
                        ins.arguments.iter_mut().for_each(|r| *r += reg_start);
                    }
                    Instruction::CallInstance(ins) => {
                        ins.location.set_inlined_call_id(inline_offset);
                        ins.register += reg_start;
                        ins.receiver += reg_start;
                        ins.arguments.iter_mut().for_each(|r| *r += reg_start);
                    }
                    Instruction::CallExtern(ins) => {
                        ins.location.set_inlined_call_id(inline_offset);
                        ins.register += reg_start;
                        ins.arguments.iter_mut().for_each(|r| *r += reg_start);
                    }
                    Instruction::CallDynamic(ins) => {
                        ins.location.set_inlined_call_id(inline_offset);
                        ins.register += reg_start;
                        ins.receiver += reg_start;
                        ins.arguments.iter_mut().for_each(|r| *r += reg_start);
                    }
                    Instruction::CallClosure(ins) => {
                        ins.location.set_inlined_call_id(inline_offset);
                        ins.register += reg_start;
                        ins.receiver += reg_start;
                        ins.arguments.iter_mut().for_each(|r| *r += reg_start);
                    }
                    Instruction::CallDropper(ins) => {
                        ins.location.set_inlined_call_id(inline_offset);
                        ins.register += reg_start;
                        ins.receiver += reg_start;
                    }
                    Instruction::CallBuiltin(ins) => {
                        ins.location.set_inlined_call_id(inline_offset);
                        ins.register += reg_start;
                        ins.arguments.iter_mut().for_each(|r| *r += reg_start);
                    }
                    Instruction::Send(ins) => {
                        ins.location.set_inlined_call_id(inline_offset);
                        ins.receiver += reg_start;
                        ins.arguments.iter_mut().for_each(|r| *r += reg_start);
                    }
                    Instruction::GetField(ins) => {
                        ins.location.set_inlined_call_id(inline_offset);
                        ins.register += reg_start;
                        ins.receiver += reg_start;
                    }
                    Instruction::SetField(ins) => {
                        ins.location.set_inlined_call_id(inline_offset);
                        ins.receiver += reg_start;
                        ins.value += reg_start;
                    }
                    Instruction::CheckRefs(ins) => {
                        ins.location.set_inlined_call_id(inline_offset);
                        ins.register += reg_start;
                    }
                    Instruction::Drop(ins) => {
                        ins.location.set_inlined_call_id(inline_offset);
                        ins.register += reg_start;
                    }
                    Instruction::Free(ins) => {
                        ins.location.set_inlined_call_id(inline_offset);
                        ins.register += reg_start;
                    }
                    Instruction::Borrow(ins) => {
                        ins.location.set_inlined_call_id(inline_offset);
                        ins.register += reg_start;
                        ins.value += reg_start;
                    }
                    Instruction::Increment(ins) => {
                        ins.location.set_inlined_call_id(inline_offset);
                        ins.register += reg_start;
                    }
                    Instruction::Decrement(ins) => {
                        ins.location.set_inlined_call_id(inline_offset);
                        ins.register += reg_start;
                    }
                    Instruction::IncrementAtomic(ins) => {
                        ins.location.set_inlined_call_id(inline_offset);
                        ins.register += reg_start;
                    }
                    Instruction::DecrementAtomic(ins) => {
                        ins.location.set_inlined_call_id(inline_offset);
                        ins.register += reg_start;
                        ins.if_true += blk_start;
                        ins.if_false += blk_start;
                    }
                    Instruction::Allocate(ins) => {
                        ins.location.set_inlined_call_id(inline_offset);
                        ins.register += reg_start;
                    }
                    Instruction::Spawn(ins) => {
                        ins.location.set_inlined_call_id(inline_offset);
                        ins.register += reg_start;
                    }
                    Instruction::GetConstant(ins) => {
                        ins.location.set_inlined_call_id(inline_offset);
                        ins.register += reg_start;
                    }
                    Instruction::Cast(ins) => {
                        ins.location.set_inlined_call_id(inline_offset);
                        ins.register += reg_start;
                        ins.source += reg_start;
                    }
                    Instruction::Pointer(ins) => {
                        ins.location.set_inlined_call_id(inline_offset);
                        ins.register += reg_start;
                        ins.value += reg_start;
                    }
                    Instruction::ReadPointer(ins) => {
                        ins.location.set_inlined_call_id(inline_offset);
                        ins.register += reg_start;
                        ins.pointer += reg_start;
                    }
                    Instruction::WritePointer(ins) => {
                        ins.location.set_inlined_call_id(inline_offset);
                        ins.pointer += reg_start;
                        ins.value += reg_start;
                    }
                    Instruction::FieldPointer(ins) => {
                        ins.location.set_inlined_call_id(inline_offset);
                        ins.register += reg_start;
                        ins.receiver += reg_start;
                    }
                    Instruction::MethodPointer(ins) => {
                        ins.location.set_inlined_call_id(inline_offset);
                        ins.register += reg_start;
                    }
                    Instruction::SizeOf(ins) => {
                        ins.location.set_inlined_call_id(inline_offset);
                        ins.register += reg_start;
                    }
                    Instruction::Preempt(ins) => {
                        ins.location.set_inlined_call_id(inline_offset);
                    }
                    Instruction::Finish(ins) => {
                        ins.location.set_inlined_call_id(inline_offset);
                    }
                }
            }

            // If the block ended in a return instruction, we replace it with a
            // goto that jumps to the block that occurs _after_ the inlined
            // code.
            if let Some(location) = add_goto {
                // Reserve the exact amount necessary so we don't allocate more
                // than necessary.
                block.instructions.reserve_exact(1);
                block.instructions.push(Instruction::Goto(Box::new(Goto {
                    block: after_call,
                    location,
                })));
                caller.body.add_edge(BlockId(blk_idx), after_call);
            }
        }

        // At this point the call instruction is guaranteed to be the last
        // instruction in the basic block, so we can just pop it from the block.
        caller.body.block_mut(self.block).instructions.pop();

        for (&from, to) in self.arguments.iter().zip(callee.arguments) {
            caller.body.block_mut(self.block).move_register(to, from, loc);
        }

        let inline_start = start_id + blk_start;

        // Fix up the successor and predecossor blocks of the call block and
        // its successor blocks, ensuring the CFG remains correct.
        let succ = caller.body.block_mut(self.block).take_successors();

        for id in succ {
            caller.body.remove_predecessor(id, self.block);
            caller.body.add_edge(after_call, id);
        }

        caller.body.block_mut(self.block).goto(inline_start, loc);
        caller.body.add_edge(self.block, inline_start);
    }
}

struct InlineNode {
    /// The inlining weight of the method.
    weight: u16,

    /// The number of call sites of the method.
    calls: u16,

    /// A flag indicating the method is a recursive method.
    recursive: bool,

    /// The graph/MIR indexes of the methods called by this method.
    callees: Vec<usize>,

    /// An integer used to prevent tracking duplicate callees.
    ///
    /// This value has no meaning once the graph is built.
    epoch: u32,

    /// A boolean indicating the method returns a value or not.
    returns: bool,
}

/// A graph/collection of statistics for each method that we may consider for
/// inlining.
///
/// The nodes in this graph are indexed using the raw `MethodId` values. This
/// means the graph will be a bit larger than the total number of MIR methods,
/// but it saves us a lot of hashing.
struct InlineGraph {
    /// The nodes in the call graph.
    nodes: Vec<InlineNode>,

    /// A mapping of raw `MethodId` values to their MIR/graph indexes.
    ///
    /// Missing slots are indicated using `u32::MAX`, which is always greater
    /// than the maximum number of methods allowed. This ensures that if we ever
    /// try to use the value of an empty slot for the `nodes` field, we get an
    /// out of bounds panic opposed to the wrong data.
    indexes: Vec<usize>,
}

impl InlineGraph {
    fn new(db: &Database, mir: &Mir) -> InlineGraph {
        let mut epoch = 0_u32;
        let mut nodes: Vec<_> = mir
            .methods
            .values()
            .map(|m| InlineNode {
                weight: 0,
                calls: 0,
                recursive: false,
                callees: Vec::new(),
                epoch: 0,
                returns: m.id.returns_value(db),
            })
            .collect();

        let mut indexes = vec![u32::MAX as usize; db.number_of_methods()];

        for (idx, method) in mir.methods.values().enumerate() {
            indexes[method.id.0 as usize] = idx;
        }

        for (idx, method) in mir.methods.values().enumerate() {
            let mut callees = Vec::new();

            for block in &method.body.blocks {
                for ins in &block.instructions {
                    // We only need to concern ourselves with static dispatch
                    // call sites here, as these are the only kind of call sites
                    // we can inline.
                    let to_id = match ins {
                        Instruction::CallInstance(i) => i.method,
                        Instruction::CallStatic(i) => i.method,
                        _ => continue,
                    };

                    let idx = indexes[to_id.0 as usize];
                    let data = &mut nodes[idx];

                    data.calls += 1;

                    // If we encounter the same callee in the same method, we
                    // don't want nor need to collect the edge data again. In
                    // the event a method has more calls than the epoch can
                    // count we _do_ track duplicates, but that's OK because the
                    // graph routines can handle that; we're just trying to
                    // reduce the amount of work.
                    if data.epoch == epoch {
                        continue;
                    } else {
                        data.epoch = epoch;
                    }

                    // Tarjan's algorithm doesn't handle self recursive nodes,
                    // but those are trivial to detect so we handle them right
                    // away here.
                    if method.id == to_id {
                        data.recursive = true;
                    } else {
                        callees.push(idx);
                    }
                }
            }

            let data = &mut nodes[idx];

            data.weight = method_weight(db, method);
            data.callees = callees;
            epoch = epoch.wrapping_add(1);
        }

        InlineGraph { nodes, indexes }
    }

    /// Returns the tails (= inner-most node) of the strongly connected
    /// components, and also marks recursive methods as such.
    ///
    /// Self recursive methods are handled when constructing the graph, so this
    /// method only flags methods that are recursive indirectly
    /// (e.g. `A -> B -> A`).
    ///
    /// The implementation here is based on the iterative implementation of
    /// Tarjan's strongly connected components algorithm as found at pages 9-10
    /// of the thesis
    /// "Verification of an iterative implementation of Tarjan’s algorithm for
    /// Strongly Connected Components using Dafny" found at
    /// <https://research.tue.nl/en/studentTheses/verification-of-an-iterative-implementation-of-tarjans-algorithm->.
    fn strongly_connected_components(&mut self) -> Vec<usize> {
        let size = self.nodes.len();
        let mut result = Vec::new();
        let mut stack = Vec::new();
        let mut on_stack = vec![false; size];
        let mut low = vec![0_usize; size];
        let mut ids = vec![0_usize; size];
        let mut id = 0;

        for root in 0..size {
            if low[root] > 0 {
                continue;
            }

            let mut work = vec![(root, 0)];

            while let Some((node, edge_idx)) = work.pop() {
                if edge_idx == 0 {
                    // Increment first since we use 0 to signal a lack of a
                    // value in the low and ID maps.
                    id += 1;

                    ids[node] = id;
                    low[node] = id;
                    stack.push(node);
                    on_stack[node] = true;
                }

                let mut recurse = false;
                let edges = &self.nodes[node].callees;

                for next_edge_idx in edge_idx..edges.len() {
                    let next_edge = edges[next_edge_idx];

                    if low[next_edge] == 0 {
                        work.push((node, next_edge_idx + 1));
                        work.push((next_edge, 0));
                        recurse = true;
                        break;
                    } else if on_stack[next_edge] {
                        low[node] = min(low[node], ids[next_edge]);
                    }
                }

                if recurse {
                    continue;
                }

                if low[node] == ids[node] {
                    let mut tail = None;
                    let mut recursive = true;

                    while let Some(connected) = stack.pop() {
                        on_stack[connected] = false;

                        // If a call in the chain never returns then we never
                        // inline it, and at runtime the recursion is "broken"
                        // by the program terminating.
                        //
                        // An example of where this is important is `Int`: when
                        // operators ovewflor, they call a method to present the
                        // error and passes the LHS and RHS of the operation.
                        // These values are then formatted to a String, which
                        // may result in the code recursing back into the
                        // operator.
                        //
                        // The check here ensures that we don't end up flagging
                        // such operators as "recursive" just because the
                        // overflow handling.
                        if !self.nodes[connected].returns {
                            recursive = false;
                        }

                        if tail.is_none() {
                            result.push(connected);
                        }

                        if connected == node {
                            break;
                        } else if tail.is_none() {
                            // If the SCC contains more than one node, it means
                            // it's part of a recursive call, so we flag it
                            // accordingly.
                            tail = Some(connected);
                        }
                    }

                    if let Some(node) = tail {
                        self.nodes[node].recursive = recursive;
                    }
                }

                if let Some((last, _)) = work.last().cloned() {
                    low[last] = min(low[last], low[node]);
                }
            }
        }

        result
    }

    fn weight_by_index(&self, index: usize) -> u16 {
        self.nodes[index].weight
    }

    fn set_weight(&mut self, index: usize, weight: u16) {
        self.nodes[index].weight = weight;
    }

    fn node(&self, id: MethodId) -> &InlineNode {
        &self.nodes[self.indexes[id.0 as usize]]
    }

    fn is_recursive(&self, id: MethodId) -> bool {
        self.nodes[self.indexes[id.0 as usize]].recursive
    }

    fn returns(&self, id: MethodId) -> bool {
        self.nodes[self.indexes[id.0 as usize]].returns
    }
}

struct Call<'a> {
    id: MethodId,
    register: RegisterId,
    receiver: Option<RegisterId>,
    arguments: &'a [RegisterId],
    location: InstructionLocation,
}

/// A compiler pass that inlines static method calls into their call sites.
pub(crate) struct InlineMethod<'a, 'b, 'c> {
    state: &'a mut State,
    mir: &'b mut Mir,
    graph: &'c mut InlineGraph,

    /// The module of the caller.
    module: ModuleId,

    /// The node ID of the caller's module in the dependency graph.
    dependency_id: usize,

    /// The global MIR index of the caller.
    method: usize,
}

impl<'a, 'b, 'c> InlineMethod<'a, 'b, 'c> {
    pub(crate) fn run_all(state: &'a mut State, mir: &'a mut Mir) {
        let mut graph = InlineGraph::new(&state.db, mir);
        let comps = graph.strongly_connected_components();

        // This ensures that:
        //
        // 1. We process methods in a deterministic order, as the MIR methods
        //    are sorted and the SCCs don't change if the source code stays the
        //    same.
        // 2. We process methods bottom-up, reducing the amount of redundant
        //    inlining work.
        for index in comps {
            let module = mir.methods[index].id.source_module(&state.db);
            let dependency_id =
                state.dependency_graph.add_module(module.name(&state.db));

            InlineMethod {
                state,
                mir,
                method: index,
                dependency_id,
                module,
                graph: &mut graph,
            }
            .run();
        }
    }

    fn run(mut self) -> bool {
        let mut inlined = false;

        loop {
            let mut work = self.inline_call_sites();

            if work.is_empty() {
                break;
            } else {
                inlined = true;
            }

            // We process the work list in reverse order so that modifying the
            // basic blocks doesn't invalidate instruction indexes, ensuring we
            // only need a single pass to determine which instructions need
            // inlining.
            while let Some(call) = work.pop() {
                // We can't both mutably borrow the method we want to inline
                // _into_ and immutably borrow the source method, so we have to
                // clone the necessary data ahead of time and then merge that
                // into the caller.
                let callee = {
                    let m = self.mir.methods.get(&call.id).unwrap();

                    Callee {
                        registers: m.registers.clone(),
                        body: m.body.clone(),
                        arguments: m.arguments.clone(),
                        inlined_calls: m.inlined_calls.clone(),
                    }
                };

                let caller = &mut self.mir.methods[self.method];
                let after = caller.body.add_block();

                // Blocks are guaranteed to have a terminator instruction at this
                // point, meaning a call to inline is never the last instruction in
                // the block.
                let mut after_ins = caller
                    .body
                    .block_mut(call.block)
                    .instructions
                    .split_off(call.instruction + 1);

                caller
                    .body
                    .block_mut(after)
                    .instructions
                    .append(&mut after_ins);

                call.inline_into(caller, callee, after);
            }
        }

        inlined
    }

    fn inline_call_sites(&mut self) -> Vec<CallSite> {
        let caller = &self.mir.methods[self.method];
        let mut caller_weight = self.graph.weight_by_index(self.method);
        let mut sites = Vec::new();
        let mut inlined = HashSet::new();

        for (blk_idx, block) in caller.body.blocks.iter().enumerate() {
            for (ins_idx, ins) in block.instructions.iter().enumerate() {
                let callee = match self.inline_result(caller_weight, ins) {
                    Some(call) => {
                        let weight = self.graph.node(call.id).weight;

                        sites.push(CallSite::new(
                            call.register,
                            BlockId(blk_idx),
                            ins_idx,
                            call.receiver,
                            call.arguments,
                            self.mir.methods.get(&call.id).unwrap(),
                            call.location,
                        ));

                        caller_weight = caller_weight.saturating_add(weight);
                        call.id
                    }
                    _ => continue,
                };

                // The calling module might not directly depend on the module
                // that defines the method. To ensure incremental caches are
                // flushed when needed, we record the dependency of the caller's
                // module on the callee's module.
                let callee_mod_id = callee.source_module(&self.state.db);
                let callee_mod_node = self
                    .state
                    .dependency_graph
                    .add_module(callee_mod_id.name(&self.state.db));

                self.state
                    .dependency_graph
                    .add_depending(self.dependency_id, callee_mod_node);

                // Even if the dependencies remain the same, it's possible that
                // the state of inlining changes. For example, two methods
                // called may originate from the same module, but the decision
                // to inline one of them may change between compilations. As
                // such we need to not only record the module dependencies, but
                // also the list of inlined methods.
                if self.module != callee_mod_id {
                    inlined.insert(callee);
                }
            }
        }

        self.graph.set_weight(self.method, caller_weight);
        self.mir
            .modules
            .get_mut(&self.module)
            .unwrap()
            .inlined_methods
            .extend(inlined);

        sites
    }

    fn inline_result<'ins>(
        &self,
        caller_weight: u16,
        instruction: &'ins Instruction,
    ) -> Option<Call<'ins>> {
        let call = match instruction {
            Instruction::CallStatic(ins) => Call {
                id: ins.method,
                register: ins.register,
                receiver: None,
                arguments: &ins.arguments,
                location: ins.location,
            },
            Instruction::CallInstance(ins) => Call {
                id: ins.method,
                register: ins.register,
                receiver: Some(ins.receiver),
                arguments: &ins.arguments,
                location: ins.location,
            },
            _ => return None,
        };

        // If either the caller or callee never returns then there's no point in
        // inlining the methods, because the program will terminate after the
        // call, which in turn suggests a branch/path we'll almost never take.
        if !self.graph.nodes[self.method].returns
            || !self.graph.returns(call.id)
        {
            return None;
        }

        // When encountering a recursive method we simply don't inline it at
        // all, as this is likely a waste of the inline budget better served
        // inlining other methods.
        if self.graph.is_recursive(call.id) {
            return None;
        }

        let node = self.graph.node(call.id);
        let inline = match call.id.inline(&self.state.db) {
            Inline::Always => true,
            Inline::Infer => {
                node.weight == 0
                    || (node.calls <= INLINE_ANYWAY_CALL_COUNT)
                    || (caller_weight.saturating_add(node.weight) <= MAX_WEIGHT)
            }
            Inline::Never => false,
        };

        inline.then_some(call)
    }
}
