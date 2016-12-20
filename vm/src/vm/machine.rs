//! Virtual Machine for running instructions

use std::io::{self, Write};
use std::path::PathBuf;
use std::thread;
use std::sync::mpsc::channel;

use binding::RcBinding;
use bytecode_parser;
use call_frame::CallFrame;
use compiled_code::RcCompiledCode;
use config::Config;
use gc::thread::Thread as GcThread;
use gc::request::Request as GcRequest;
use vm::instruction::{Instruction, INSTRUCTION_MAPPING};
use object_pointer::ObjectPointer;
use object_value;
use vm::state::RcState;
use vm::action::Action;
use vm::instructions::result::InstructionResult;
use process::{RcProcess, Process};
use execution_context::ExecutionContext;
use thread::{RcThread, JoinHandle as ThreadJoinHandle};

pub struct Machine {
    pub state: RcState,
}

impl Machine {
    pub fn new(state: RcState) -> Machine {
        Machine { state: state }
    }

    pub fn config(&self) -> &Config {
        &self.state.config
    }

    /// Starts the main thread
    ///
    /// This requires a RcCompiledCode to run. Calling this method will block
    /// execution as the main thread is executed in the same OS thread as the
    /// caller of this function is operating in.
    pub fn start(&self, code: RcCompiledCode) -> Result<(), ()> {
        for _ in 0..self.config().process_threads {
            self.start_thread();
        }

        for _ in 0..self.config().gc_threads {
            self.start_gc_thread()
        }

        let thread = self.allocate_main_thread();
        let (_, process) =
            self.allocate_process(code, self.state.top_level.clone());

        thread.schedule(process);

        self.run_thread(thread);

        *read_lock!(self.state.exit_status)
    }

    fn allocate_thread(&self, handle: Option<ThreadJoinHandle>) -> RcThread {
        write_lock!(self.state.threads).add(handle)
    }

    fn allocate_main_thread(&self) -> RcThread {
        write_lock!(self.state.threads).add_main_thread()
    }

    /// Allocates a new process and returns the PID and Process structure.
    pub fn allocate_process(&self,
                            code: RcCompiledCode,
                            self_obj: ObjectPointer)
                            -> (usize, RcProcess) {
        let mut processes = write_lock!(self.state.processes);
        let pid = processes.reserve_pid();
        let process = Process::from_code(pid,
                                         code,
                                         self_obj,
                                         self.state.global_allocator.clone());

        processes.add(pid, process.clone());

        (pid, process)
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

    fn run(&self, thread: RcThread, process: RcProcess) -> Result<(), String> {
        let mut reductions = self.config().reductions;

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

                match func(self, &process, &code, instruction)? {
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

            self.gc_safepoint(&thread, &process);

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

    /// Prints a VM backtrace of a given thread with a message.
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

        *write_lock!(self.state.exit_status) = Err(());
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

            for directory in self.config().directories.iter() {
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

        ensure_compiled_code!(instruction, method_obj);

        let method_code = method_obj.value.as_compiled_code();

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

                ensure_arrays!(instruction, array);

                for value in array.value.as_array() {
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

    /// Starts a new thread.
    fn start_thread(&self) -> RcThread {
        let state_clone = self.state.clone();

        let (sender, receiver) = channel();

        let handle = thread::spawn(move || {
            let thread = receiver.recv().unwrap();
            let vm = Machine::new(state_clone);

            vm.run_thread(thread);
        });

        let thread = self.allocate_thread(Some(handle));

        sender.send(thread.clone()).unwrap();

        thread
    }

    /// Starts a new GC thread
    fn start_gc_thread(&self) {
        let state_clone = self.state.clone();

        thread::spawn(move || {
            let mut gc_thread = GcThread::new(state_clone);

            gc_thread.run();
        });
    }

    /// Spawns a new process.
    pub fn spawn_process(&self,
                         process: &RcProcess,
                         code: RcCompiledCode,
                         register: usize) {
        let (pid, new_proc) =
            self.allocate_process(code, self.state.top_level.clone());

        write_lock!(self.state.threads).schedule(new_proc);

        let pid_obj = process.allocate(object_value::integer(pid as i64),
                                       self.state.integer_prototype.clone());

        process.set_register(register, pid_obj);
    }

    /// Start a thread's execution loop.
    fn run_thread(&self, thread: RcThread) {
        while !thread.should_stop() {
            // Bail out if any of the other threads errored.
            if read_lock!(self.state.exit_status).is_err() {
                break;
            }

            // Terminate gracefully once the main thread has processed its
            // process queue.
            if thread.main_can_terminate() {
                write_lock!(self.state.threads).stop();
                break;
            }

            thread.wait_for_work();

            let proc_opt = thread.pop_process();

            // A thread may be woken up (e.g. due to a VM error) without there
            // being work available.
            if proc_opt.is_none() {
                continue;
            }

            let process = proc_opt.unwrap();

            match self.run(thread.clone(), process.clone()) {
                Ok(_) => {
                    if process.should_suspend_for_gc() {
                        process.suspend_for_gc();
                        thread.remember_process(process.clone());
                    } else if process.should_be_rescheduled() {
                        thread.schedule(process);
                    } else {
                        process.finished();

                        write_lock!(self.state.processes).remove(process);
                    }
                }
                Err(message) => {
                    self.error(process, message);

                    write_lock!(self.state.threads).stop();
                }
            }
        }
    }

    /// Checks if a garbage collection run should be scheduled for the given
    /// process.
    fn gc_safepoint(&self, thread: &RcThread, process: &RcProcess) {
        if process.gc_is_scheduled() {
            return;
        }

        let request_opt = if process.should_collect_young_generation() {
            Some(GcRequest::heap(thread.clone(), process.clone()))
        } else if process.should_collect_mailbox() {
            Some(GcRequest::mailbox(thread.clone(), process.clone()))
        } else {
            None
        };

        if let Some(request) = request_opt {
            process.gc_scheduled();

            self.state.gc_requests.push(request);
        }
    }
}
