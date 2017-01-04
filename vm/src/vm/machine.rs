//! Virtual Machine for running instructions

use std::io::{self, Write};
use std::path::PathBuf;

use binding::RcBinding;
use bytecode_parser;
use call_frame::CallFrame;
use compiled_code::RcCompiledCode;
use execution_context::ExecutionContext;
use gc::request::Request as GcRequest;
use object_pointer::ObjectPointer;
use object_value;
use process::{RcProcess, Process};
use pool::JoinGuard as PoolJoinGuard;
use vm::action::Action;
use vm::instruction::{Instruction, INSTRUCTION_MAPPING};
use vm::instructions::result::InstructionResult;
use vm::state::RcState;

pub struct Machine {
    pub state: RcState,
}

impl Machine {
    pub fn new(state: RcState) -> Machine {
        Machine { state: state }
    }

    /// Starts the VM
    ///
    /// This method will block the calling thread until it returns.
    pub fn start(&self, code: RcCompiledCode) -> Result<(), ()> {
        let process_pool_guard = self.start_process_threads();
        let gc_pool_guard = self.start_gc_threads();

        let main_process =
            self.allocate_process(code, self.state.top_level.clone());

        self.state.process_pool.schedule(main_process);

        if process_pool_guard.join().is_err() {
            return Err(());
        }

        if gc_pool_guard.join().is_err() {
            return Err(());
        }

        *self.state.exit_status.lock()
    }

    /// Starts the threads that will execute processes.
    fn start_process_threads(&self) -> PoolJoinGuard<()> {
        let machine = Machine::new(self.state.clone());

        self.state.process_pool.run(move |process| {
            match machine.run(&process) {
                Ok(_) => {
                    if process.should_suspend_for_gc() {
                        process.suspend_for_gc();
                    } else if process.should_be_rescheduled() {
                        machine.state.process_pool.schedule(process.clone());
                    } else {
                        let is_main = process.is_main();

                        process.finished();

                        write_lock!(machine.state.processes).remove(process);

                        // Terminate once the main process has finished
                        // execution.
                        if is_main {
                            machine.terminate();
                        }
                    }
                }
                Err(message) => machine.error(process, message),
            }
        })
    }

    /// Starts the garbage collection threads.
    fn start_gc_threads(&self) -> PoolJoinGuard<()> {
        self.state.gc_pool.run(move |request| request.perform())
    }

    fn terminate(&self) {
        self.state.process_pool.terminate();
        self.state.gc_pool.terminate();
    }

    /// Allocates a new process and returns the PID and Process structure.
    pub fn allocate_process(&self,
                            code: RcCompiledCode,
                            self_obj: ObjectPointer)
                            -> RcProcess {
        let mut processes = write_lock!(self.state.processes);
        let pid = processes.reserve_pid();
        let process = Process::from_code(pid,
                                         code,
                                         self_obj,
                                         self.state.global_allocator.clone());

        processes.add(pid, process.clone());

        process
    }

    pub fn allocate_method(&self,
                           process: &RcProcess,
                           receiver: &ObjectPointer,
                           code: RcCompiledCode)
                           -> ObjectPointer {
        let value = object_value::compiled_code(code);
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

    fn run(&self, process: &RcProcess) -> Result<(), String> {
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

                match func(self, process, &code, instruction)? {
                    Action::Goto(new_index) => index = new_index,
                    Action::Return => break,
                    Action::EnterContext => {
                        context.instruction_index = index;

                        continue 'exec_loop;
                    }
                    Action::Suspend => {
                        context.instruction_index = index - 1;
                        process.suspend();

                        return Ok(());
                    }
                    _ => {}
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
            process.pop_call_frame();

            // The underlying ExecutionContext is no longer available at this
            // point. Rust however is not aware of this due to the use of the
            // LocalData structure in Process.
            drop(context);

            self.gc_safepoint(&process);

            if process.should_suspend_for_gc() {
                return Ok(());
            }

            // Reduce once we've exhausted all the instructions in a context.
            if reductions > 0 {
                reductions -= 1;
            } else {
                process.suspend();

                return Ok(());
            }
        } // loop

        Ok(())
    }

    /// Prints a VM backtrace of a given process with a message.
    fn error(&self, process: RcProcess, error: String) {
        let mut stderr = io::stderr();
        let mut message = format!("A fatal VM error occurred in process {}:",
                                  process.pid);

        message.push_str(&format!("\n\n{}\n\nCall stack:\n\n", error));

        for frame in process.call_frame().call_stack() {
            message.push_str(&format!("{} line {} in {}\n",
                                      frame.file(),
                                      frame.line,
                                      frame.name()));
        }

        stderr.write(message.as_bytes()).unwrap();
        stderr.flush().unwrap();

        *self.state.exit_status.lock() = Err(());

        self.terminate();
    }

    /// Schedules the execution of a new CompiledCode.
    pub fn schedule_code(&self,
                         process: RcProcess,
                         code: RcCompiledCode,
                         self_obj: ObjectPointer,
                         args: &Vec<ObjectPointer>,
                         binding: Option<RcBinding>,
                         register: usize) {
        let context = if let Some(rc_bind) = binding {
            ExecutionContext::with_binding(rc_bind, code.clone(), Some(register))
        } else {
            ExecutionContext::with_object(self_obj, code.clone(), Some(register))
        };

        let frame = CallFrame::from_code(code);

        process.push_context(context);
        process.push_call_frame(frame);

        for (index, arg) in args.iter().enumerate() {
            process.set_local(index, arg.clone());
        }
    }

    /// Runs a bytecode file.
    pub fn run_file(&self,
                    path_str: &String,
                    process: &RcProcess,
                    instruction: &Instruction,
                    register: usize)
                    -> InstructionResult {
        process.advance_line(instruction.line);

        {
            let mut executed = write_lock!(self.state.executed_files);

            if executed.contains(path_str) {
                return Ok(Action::None);
            } else {
                executed.insert(path_str.clone());
            }
        }

        let mut input_path = PathBuf::from(path_str);

        if input_path.is_relative() {
            let mut found = false;

            for directory in self.state.config.directories.iter() {
                let full_path = directory.join(path_str);

                if full_path.exists() {
                    input_path = full_path;
                    found = true;

                    break;
                }
            }

            if !found {
                return Err(format!("No file found for {}", path_str));
            }
        }

        let input_path_str = input_path.to_str().unwrap();

        match bytecode_parser::parse_file(input_path_str) {
            Ok(body) => {
                let self_obj = self.state.top_level.clone();

                self.schedule_code(process.clone(),
                                   body,
                                   self_obj,
                                   &Vec::new(),
                                   None,
                                   register);

                process.pop_call_frame();

                Ok(Action::EnterContext)
            }
            Err(err) => {
                Err(format!("Failed to parse {}: {:?}", input_path_str, err))
            }
        }
    }

    /// Sends a message to an object.
    pub fn send_message(&self,
                        name: &String,
                        process: &RcProcess,
                        instruction: &Instruction)
                        -> InstructionResult {
        // Advance the line number so error messages contain the correct frame
        // pointing to the call site.
        process.advance_line(instruction.line);

        let register = instruction.arg(0)?;
        let receiver_ptr = process.get_register(instruction.arg(1)?)?;
        let rest_arg = instruction.arg(3)? == 1;

        let method_ptr = receiver_ptr.get()
            .lookup_method(name)
            .ok_or_else(|| format!("undefined method \"{}\"", name))?;

        let method_obj = method_ptr.get();
        let method_code = method_obj.value.as_compiled_code()?;

        // Argument handling
        let arg_count = instruction.arguments.len() - 4;
        let tot_args = method_code.arguments as usize;
        let req_args = method_code.required_arguments as usize;

        let mut arguments =
            self.collect_arguments(process.clone(), instruction, 4, arg_count)?;

        // Unpack the last argument if it's a rest argument
        if rest_arg {
            if let Some(last_arg) = arguments.pop() {
                let array = last_arg.get();

                for value in array.value.as_array()? {
                    arguments.push(value.clone());
                }
            }
        }

        // If the method defines a rest argument we'll pack any excessive
        // arguments into a single array.
        if method_code.rest_argument && arguments.len() > tot_args {
            let rest_count = arguments.len() - tot_args;
            let mut rest = Vec::new();

            for obj in arguments[arguments.len() - rest_count..].iter() {
                rest.push(obj.clone());
            }

            arguments.truncate(tot_args);

            let rest_array = process.allocate(object_value::array(rest),
                                              self.state.array_prototype.clone());

            arguments.push(rest_array);
        } else if method_code.rest_argument && arguments.len() == 0 {
            let rest_array = process.allocate(object_value::array(Vec::new()),
                                              self.state.array_prototype.clone());

            arguments.push(rest_array);
        }

        if arguments.len() > tot_args && !method_code.rest_argument {
            return Err(format!("{} accepts up to {} arguments, but {} \
                                arguments were given",
                               name,
                               method_code.arguments,
                               arguments.len()));
        }

        if arguments.len() < req_args {
            return Err(format!("{} requires {} arguments, but {} arguments \
                                were given",
                               name,
                               method_code.required_arguments,
                               arguments.len()));
        }

        self.schedule_code(process.clone(),
                           method_code,
                           receiver_ptr.clone(),
                           &arguments,
                           None,
                           register);

        process.pop_call_frame();

        Ok(Action::EnterContext)
    }

    /// Collects a set of arguments from an instruction.
    pub fn collect_arguments(&self,
                             process: RcProcess,
                             instruction: &Instruction,
                             offset: usize,
                             amount: usize)
                             -> Result<Vec<ObjectPointer>, String> {
        let mut args: Vec<ObjectPointer> = Vec::new();

        for index in offset..(offset + amount) {
            let arg_index = instruction.arg(index)?;

            args.push(process.get_register(arg_index)?);
        }

        Ok(args)
    }

    /// Spawns a new process.
    pub fn spawn_process(&self,
                         process: &RcProcess,
                         code: RcCompiledCode,
                         register: usize) {
        let new_proc = self.allocate_process(code, self.state.top_level.clone());
        let new_pid = new_proc.pid;

        self.state.process_pool.schedule(new_proc);

        let pid_obj = process.allocate(object_value::integer(new_pid as i64),
                                       self.state.integer_prototype.clone());

        process.set_register(register, pid_obj);
    }

    /// Checks if a garbage collection run should be scheduled for the given
    /// process.
    fn gc_safepoint(&self, process: &RcProcess) {
        if process.gc_is_scheduled() {
            return;
        }

        let request_opt = if process.should_collect_young_generation() {
            Some(GcRequest::heap(self.state.clone(), process.clone()))
        } else if process.should_collect_mailbox() {
            Some(GcRequest::mailbox(self.state.clone(), process.clone()))
        } else {
            None
        };

        if let Some(request) = request_opt {
            process.gc_scheduled();

            self.state.gc_pool.schedule(request);
        }
    }
}
