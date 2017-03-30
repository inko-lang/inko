//! Virtual Machine for running instructions
use binding::Binding;
use block::Block;
use compiled_code::RcCompiledCode;
use execution_context::ExecutionContext;
use file_registry::{FileRegistry, RcFileRegistry};
use gc::request::Request as GcRequest;
use object_pointer::ObjectPointer;
use object_value;
use process::{RcProcess, Process};
use pool::JoinGuard as PoolJoinGuard;
use pools::{PRIMARY_POOL, SECONDARY_POOL};
use vm::action::Action;
use vm::instruction::{Instruction, INSTRUCTION_MAPPING};
use vm::state::RcState;

#[derive(Clone)]
pub struct Machine {
    pub state: RcState,
    pub file_registry: RcFileRegistry,
}

impl Machine {
    /// Creates a new Machine with various fields set to their defaults.
    pub fn default(state: RcState) -> Self {
        let file_registry = FileRegistry::with_rc(state.clone());

        Machine::new(state, file_registry)
    }

    pub fn new(state: RcState, file_registry: RcFileRegistry) -> Self {
        Machine {
            state: state,
            file_registry: file_registry,
        }
    }

    /// Starts the VM
    ///
    /// This method will block the calling thread until it returns.
    pub fn start(&self, code: RcCompiledCode) -> Result<(), String> {
        let primary_guard = self.start_primary_threads();
        let gc_pool_guard = self.start_gc_threads();

        self.start_secondary_threads();

        let main_process = self.allocate_process(PRIMARY_POOL, code)?;

        self.state.process_pools.schedule(main_process);

        if primary_guard.join().is_err() {
            self.terminate();

            return Err("Failed to join the primary process pool".to_string());
        }

        if gc_pool_guard.join().is_err() {
            self.terminate();

            return Err("Failed to join the GC pool".to_string());
        }

        self.state.exit_status.lock().clone()
    }

    fn start_primary_threads(&self) -> PoolJoinGuard<()> {
        let machine = self.clone();
        let pool = self.state.process_pools.get(PRIMARY_POOL).unwrap();

        pool.run(move |process| machine.run(&process))
    }

    fn start_secondary_threads(&self) {
        let machine = self.clone();
        let pool = self.state.process_pools.get(SECONDARY_POOL).unwrap();

        pool.run(move |process| machine.run(&process));
    }

    /// Starts the garbage collection threads.
    fn start_gc_threads(&self) -> PoolJoinGuard<()> {
        self.state.gc_pool.run(move |request| request.perform())
    }

    fn terminate(&self) {
        self.state.process_pools.terminate();
        self.state.gc_pool.terminate();
    }

    /// Allocates a new process and returns the PID and Process structure.
    pub fn allocate_process(&self,
                            pool_id: usize,
                            code: RcCompiledCode)
                            -> Result<RcProcess, String> {
        let mut process_table = write_lock!(self.state.process_table);

        let pid = process_table.reserve()
            .ok_or_else(|| "No PID could be reserved".to_string())?;

        let process = Process::from_code(pid,
                                         pool_id,
                                         code,
                                         self.state.global_allocator.clone());

        process_table.map(pid, process.clone());

        Ok(process)
    }

    pub fn allocate_method(&self,
                           process: &RcProcess,
                           receiver: &ObjectPointer,
                           block_ref: &Box<Block>)
                           -> ObjectPointer {
        let block = (**block_ref).clone();
        let value = object_value::block(block);
        let proto = self.state.method_prototype.clone();

        if receiver.is_permanent() {
            self.state
                .permanent_allocator
                .lock()
                .allocate_with_prototype(value, proto)
        } else {
            process.allocate(value, proto)
        }
    }

    /// Executes a single process.
    fn run(&self, process: &RcProcess) {
        let mut reductions = self.state.config.reductions;

        process.running();

        'exec_loop: loop {
            let code = process.compiled_code();
            let mut index = process.instruction_index();
            let count = code.instructions.len();

            // We're storing a &mut ExecutionContext here instead of using &mut
            // Box<ExecutionContext>. This is because such a reference (as
            // returned by context()/context_mut()) will become invalid once an
            // instruction changes the current execution context.
            let mut context = &mut **process.context_mut();

            while index < count {
                let ref instruction = code.instructions[index];
                let mapping_index = instruction.instruction_type as usize;
                let func = INSTRUCTION_MAPPING[mapping_index];

                index += 1;

                match func(self, process, &code, instruction) {
                    Ok(Action::Goto(new_index)) => index = new_index,
                    Ok(Action::Return) => break,
                    Ok(Action::EnterContext) => {
                        context.instruction_index = index;

                        continue 'exec_loop;
                    }
                    Ok(Action::Suspend) => {
                        context.instruction_index = index - 1;
                        self.reschedule(process.clone());
                        return;
                    }
                    Ok(_) => {}
                    Err(message) => {
                        self.error(process, message);

                        return;
                    }
                }
            } // while

            // Make sure that we update the stored instruction index in case we
            // need to suspend for garbage collection.
            //
            // This is important as the collector may reschedule an already
            // finished process. In that case we don't want to re-run any
            // previously executed instructions.
            context.instruction_index = index;

            // Once we're at the top-level _and_ we have no more instructions to
            // process we'll bail out of the main execution loop.
            if process.at_top_level() {
                break;
            }

            // We're not yet at the top level but we did finish running an
            // entire execution context.
            process.pop_context();

            // The underlying ExecutionContext is no longer available at this
            // point. Rust however is not aware of this due to the use of the
            // LocalData structure in Process.
            drop(context);

            if self.gc_safepoint(&process) {
                return;
            }

            // Reduce once we've exhausted all the instructions in a context.
            if reductions > 0 {
                reductions -= 1;
            } else {
                self.reschedule(process.clone());
                return;
            }
        } // loop

        process.finished();

        write_lock!(self.state.process_table).release(&process.pid);

        // Terminate once the main process has finished execution.
        if process.is_main() {
            self.terminate();
        }
    }

    /// Prints a VM backtrace of a given process with a message.
    fn error(&self, process: &RcProcess, error: String) {
        let mut message = format!("A fatal VM error occurred in process {}:",
                                  process.pid);

        message.push_str(&format!("\n\n{}\n\nCall stack:\n\n", error));

        for context in process.context().contexts() {
            message.push_str(&format!("{} line {} in {}\n",
                                      context.file(),
                                      context.line,
                                      context.name()));
        }

        *self.state.exit_status.lock() = Err(message);

        self.terminate();
    }

    /// Schedules the execution of a new Block.
    pub fn schedule_code(&self,
                         process: RcProcess,
                         block: &Box<Block>,
                         args: &Vec<ObjectPointer>,
                         register: usize) {
        let code = block.code.clone();

        let binding = Binding::with_parent(block.binding.clone(),
                                           code.locals as usize);

        let context =
            ExecutionContext::new(binding, code.clone(), Some(register));

        process.push_context(context);

        for (index, arg) in args.iter().enumerate() {
            process.set_local(index, arg.clone());
        }
    }

    /// Collects a set of arguments from an instruction.
    pub fn collect_arguments(&self,
                             process: RcProcess,
                             instruction: &Instruction,
                             offset: usize,
                             amount: usize)
                             -> Result<Vec<ObjectPointer>, String> {
        let mut args: Vec<ObjectPointer> = Vec::with_capacity(amount);

        for index in offset..(offset + amount) {
            let arg_index = instruction.arg(index)?;

            args.push(process.get_register(arg_index)?);
        }

        Ok(args)
    }

    /// Spawns a new process.
    pub fn spawn_process(&self,
                         process: &RcProcess,
                         pool_id: usize,
                         code: RcCompiledCode,
                         register: usize)
                         -> Result<(), String> {
        let new_proc = self.allocate_process(pool_id, code)?;
        let new_pid = new_proc.pid;

        self.state.process_pools.schedule(new_proc);

        process.set_register(register, ObjectPointer::integer(new_pid as i64));

        Ok(())
    }

    /// Checks if a garbage collection run should be scheduled for the given
    /// process.
    ///
    /// Returns true if a process should be suspended for garbage collection.
    fn gc_safepoint(&self, process: &RcProcess) -> bool {
        let request_opt = if process.should_collect_young_generation() {
            Some(GcRequest::heap(self.state.clone(), process.clone()))
        } else if process.should_collect_mailbox() {
            Some(GcRequest::mailbox(self.state.clone(), process.clone()))
        } else {
            None
        };

        if let Some(request) = request_opt {
            process.suspend_for_gc();
            self.state.gc_pool.schedule(request);

            true
        } else {
            false
        }
    }

    /// Reschedules a process.
    fn reschedule(&self, process: RcProcess) {
        self.state.process_pools.schedule(process);
    }
}
