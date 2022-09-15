use crate::execution_context::ExecutionContext;
use crate::image::Image;
use crate::instructions::array;
use crate::instructions::builtin_functions;
use crate::instructions::byte_array;
use crate::instructions::float;
use crate::instructions::future;
use crate::instructions::general;
use crate::instructions::integer;
use crate::instructions::module;
use crate::instructions::process;
use crate::instructions::string;
use crate::mem::{Int, Pointer, String as InkoString};
use crate::network_poller::Worker as NetworkPollerWorker;
use crate::process::{Process, ProcessPointer, TaskPointer};
use crate::runtime_error::RuntimeError;
use crate::state::State;
use bytecode::Instruction;
use bytecode::Opcode;
use std::fmt::Write;
use std::thread;

macro_rules! reset {
    ($task: expr, $loop_state: expr) => {{
        $loop_state = unsafe { LoopState::new($task) };
    }};
}

/// Handles a function that may produce an Inko panic.
macro_rules! try_panic {
    ($expr: expr, $loop_state: expr) => {{
        match $expr {
            Ok(thing) => thing,
            Err(msg) => vm_panic!(msg, $loop_state),
        }
    }};
}

macro_rules! vm_panic {
    ($message: expr, $loop_state: expr) => {{
        $loop_state.rewind();

        return Err($message);
    }};
}

/// The state of an interpreter loop, such as the context being executed.
///
/// We use a structure here so we don't need to pass around tons of variables
/// just to support suspending/resuming processes in the right place.
struct LoopState<'a> {
    instructions: &'a [Instruction],
    index: usize,
    context: &'a mut ExecutionContext,
}

impl<'a> LoopState<'a> {
    /// Returns a new LoopState for the given task.
    ///
    /// This method is unsafe as we perform multiple borrows of data related to
    /// the task. This is necessary to prevent additional/needless pointer reads
    /// in the interpreter loop. Unfortunately, there's no way of expressing
    /// this in safe Rust code, hence the use of unsafe code in this method.
    ///
    /// Because of this code, care must be taken to reset the loop state at the
    /// right time. For example, when a new method is scheduled the state must
    /// be reset; otherwise we'll continue running the current method.
    ///
    /// If there's a better way of going about this (without incurring runtime
    /// overhead) we'd love to adopt that. Unfortunately, as of July 2021 this
    /// is the best we can do :<
    unsafe fn new(mut task: TaskPointer) -> Self {
        let context = &mut *(&mut *task.context as *mut ExecutionContext);
        let method = context.method();

        // Accessing instructions using `context.method().instructions` would
        // incur a pointer read for every instruction, followed by the offset to
        // determine the current instruction. By storing the slice we can avoid
        // the pointer read.
        let instructions = &*(method.instructions.as_slice() as *const _);

        LoopState { index: context.index, instructions, context }
    }

    fn rewind(&mut self) {
        self.context.index = self.index - 1;
    }

    fn save(&mut self) {
        self.context.index = self.index;
    }
}

pub struct Machine<'a> {
    /// The shared virtual machine state, such as the process pools and built-in
    /// types.
    pub(crate) state: &'a State,
}

impl<'a> Machine<'a> {
    pub(crate) fn new(state: &'a State) -> Self {
        Machine { state }
    }

    /// Boots up the VM and all its thread pools.
    ///
    /// This method blocks the calling thread until the Inko program terminates.
    pub fn boot(image: Image, arguments: &[String]) -> Result<i32, String> {
        let state = State::new(image.config, image.permanent_space, arguments);
        let entry_class = image.entry_class;
        let entry_method =
            unsafe { entry_class.get_method(image.entry_method) };

        {
            let state = state.clone();
            let _ = thread::Builder::new()
                .name("timeout worker".to_string())
                .spawn(move || state.timeout_worker.run(&state.scheduler))
                .unwrap();
        };

        let poller_guard = {
            let thread_state = state.clone();

            thread::Builder::new()
                .name("network poller".to_string())
                .spawn(move || {
                    NetworkPollerWorker::new(thread_state).run();
                })
                .unwrap()
        };

        // Starting the primary threads will block this thread, as the main
        // worker will run directly onto the current thread. As such, we must
        // start these threads last.
        let primary_guard = {
            let thread_state = state.clone();

            state.scheduler.pool.start_main(
                thread_state,
                entry_class,
                entry_method,
            )
        };

        // Joining the pools only fails in case of a panic. In this case we
        // don't want to re-panic as this clutters the error output.
        if primary_guard.join().is_err() || poller_guard.join().is_err() {
            state.set_exit_status(1);
        }

        Ok(state.current_exit_status())
    }

    pub(crate) fn run(&self, mut process: ProcessPointer) {
        // When there's no task to run, clients will try to reschedule the
        // process after sending it a message. This means we (here) don't need
        // to do anything extra.
        if let Some(task) = process.task_to_run() {
            if let Err(message) = self.run_task(process, task) {
                self.panic(process, &message);
            }
        } else if process.finish_task() {
            self.state.scheduler.schedule(process);
        }
    }

    fn run_task(
        &self,
        mut process: ProcessPointer,
        mut task: TaskPointer,
    ) -> Result<(), String> {
        let mut reductions = self.state.config.reductions as i32;
        let mut state;

        reset!(task, state);

        'ins_loop: loop {
            let ins = unsafe { state.instructions.get_unchecked(state.index) };

            state.index += 1;

            match ins.opcode {
                Opcode::Allocate => {
                    let reg = ins.arg(0);
                    let idx = ins.u32_arg(1, 2);
                    let res = general::allocate(self.state, idx);

                    state.context.set_register(reg, res);
                }
                Opcode::ArrayAllocate => {
                    let reg = ins.arg(0);
                    let res = array::allocate(self.state, task);

                    state.context.set_register(reg, res);
                }
                Opcode::GetTrue => {
                    let reg = ins.arg(0);
                    let res = Pointer::true_singleton();

                    state.context.set_register(reg, res);
                }
                Opcode::GetFalse => {
                    let reg = ins.arg(0);
                    let res = Pointer::false_singleton();

                    state.context.set_register(reg, res);
                }
                Opcode::Return => {
                    let res = state.context.get_register(ins.arg(0));

                    process.set_return_value(res);

                    // Once we're at the top-level _and_ we have no more
                    // instructions to process, we'll write the result to a
                    // future and bail out the execution loop.
                    if task.pop_context() {
                        break 'ins_loop;
                    }

                    reset!(task, state);
                }
                Opcode::Branch => {
                    let val = state.context.get_register(ins.arg(0));
                    let if_true = ins.arg(1) as usize;
                    let if_false = ins.arg(2) as usize;

                    state.index = if val == Pointer::true_singleton() {
                        if_true
                    } else {
                        if_false
                    };
                }
                Opcode::Goto => {
                    state.index = ins.arg(0) as usize;
                }
                Opcode::BranchResult => {
                    let if_ok = ins.arg(0) as usize;
                    let if_err = ins.arg(1) as usize;

                    state.index = if process.thrown() { if_err } else { if_ok };
                }
                Opcode::IntAdd => {
                    let reg = ins.arg(0);
                    let a = state.context.get_register(ins.arg(1));
                    let b = state.context.get_register(ins.arg(2));
                    let res = try_panic!(integer::add(self.state, a, b), state);

                    state.context.set_register(reg, res);
                }
                Opcode::IntDiv => {
                    let reg = ins.arg(0);
                    let a = state.context.get_register(ins.arg(1));
                    let b = state.context.get_register(ins.arg(2));
                    let res = try_panic!(integer::div(self.state, a, b), state);

                    state.context.set_register(reg, res);
                }
                Opcode::IntMul => {
                    let reg = ins.arg(0);
                    let a = state.context.get_register(ins.arg(1));
                    let b = state.context.get_register(ins.arg(2));
                    let res = try_panic!(integer::mul(self.state, a, b), state);

                    state.context.set_register(reg, res);
                }
                Opcode::IntSub => {
                    let reg = ins.arg(0);
                    let a = state.context.get_register(ins.arg(1));
                    let b = state.context.get_register(ins.arg(2));
                    let res = try_panic!(integer::sub(self.state, a, b), state);

                    state.context.set_register(reg, res);
                }
                Opcode::IntMod => {
                    let reg = ins.arg(0);
                    let a = state.context.get_register(ins.arg(1));
                    let b = state.context.get_register(ins.arg(2));
                    let res =
                        try_panic!(integer::modulo(self.state, a, b), state);

                    state.context.set_register(reg, res);
                }
                Opcode::IntBitAnd => {
                    let reg = ins.arg(0);
                    let a = state.context.get_register(ins.arg(1));
                    let b = state.context.get_register(ins.arg(2));
                    let res = integer::and(self.state, a, b);

                    state.context.set_register(reg, res);
                }
                Opcode::IntBitOr => {
                    let reg = ins.arg(0);
                    let a = state.context.get_register(ins.arg(1));
                    let b = state.context.get_register(ins.arg(2));
                    let res = integer::or(self.state, a, b);

                    state.context.set_register(reg, res);
                }
                Opcode::IntBitXor => {
                    let reg = ins.arg(0);
                    let a = state.context.get_register(ins.arg(1));
                    let b = state.context.get_register(ins.arg(2));
                    let res = integer::xor(self.state, a, b);

                    state.context.set_register(reg, res);
                }
                Opcode::IntShl => {
                    let reg = ins.arg(0);
                    let a = state.context.get_register(ins.arg(1));
                    let b = state.context.get_register(ins.arg(2));
                    let res = try_panic!(integer::shl(self.state, a, b), state);

                    state.context.set_register(reg, res);
                }
                Opcode::IntShr => {
                    let reg = ins.arg(0);
                    let a = state.context.get_register(ins.arg(1));
                    let b = state.context.get_register(ins.arg(2));
                    let res = try_panic!(integer::shr(self.state, a, b), state);

                    state.context.set_register(reg, res);
                }
                Opcode::IntLt => {
                    let reg = ins.arg(0);
                    let a = state.context.get_register(ins.arg(1));
                    let b = state.context.get_register(ins.arg(2));
                    let res = integer::lt(a, b);

                    state.context.set_register(reg, res);
                }
                Opcode::IntGt => {
                    let reg = ins.arg(0);
                    let a = state.context.get_register(ins.arg(1));
                    let b = state.context.get_register(ins.arg(2));
                    let res = integer::gt(a, b);

                    state.context.set_register(reg, res);
                }
                Opcode::IntEq => {
                    let reg = ins.arg(0);
                    let a = state.context.get_register(ins.arg(1));
                    let b = state.context.get_register(ins.arg(2));
                    let res = integer::eq(a, b);

                    state.context.set_register(reg, res);
                }
                Opcode::IntGe => {
                    let reg = ins.arg(0);
                    let a = state.context.get_register(ins.arg(1));
                    let b = state.context.get_register(ins.arg(2));
                    let res = integer::ge(a, b);

                    state.context.set_register(reg, res);
                }
                Opcode::IntLe => {
                    let reg = ins.arg(0);
                    let a = state.context.get_register(ins.arg(1));
                    let b = state.context.get_register(ins.arg(2));
                    let res = integer::le(a, b);

                    state.context.set_register(reg, res);
                }
                Opcode::FloatAdd => {
                    let reg = ins.arg(0);
                    let a = state.context.get_register(ins.arg(1));
                    let b = state.context.get_register(ins.arg(2));
                    let res = float::add(self.state, a, b);

                    state.context.set_register(reg, res);
                }
                Opcode::FloatMul => {
                    let reg = ins.arg(0);
                    let a = state.context.get_register(ins.arg(1));
                    let b = state.context.get_register(ins.arg(2));
                    let res = float::mul(self.state, a, b);

                    state.context.set_register(reg, res);
                }
                Opcode::FloatDiv => {
                    let reg = ins.arg(0);
                    let a = state.context.get_register(ins.arg(1));
                    let b = state.context.get_register(ins.arg(2));
                    let res = float::div(self.state, a, b);

                    state.context.set_register(reg, res);
                }
                Opcode::FloatSub => {
                    let reg = ins.arg(0);
                    let a = state.context.get_register(ins.arg(1));
                    let b = state.context.get_register(ins.arg(2));
                    let res = float::sub(self.state, a, b);

                    state.context.set_register(reg, res);
                }
                Opcode::FloatMod => {
                    let reg = ins.arg(0);
                    let a = state.context.get_register(ins.arg(1));
                    let b = state.context.get_register(ins.arg(2));
                    let res = float::modulo(self.state, a, b);

                    state.context.set_register(reg, res);
                }
                Opcode::FloatLt => {
                    let reg = ins.arg(0);
                    let a = state.context.get_register(ins.arg(1));
                    let b = state.context.get_register(ins.arg(2));
                    let res = float::lt(a, b);

                    state.context.set_register(reg, res);
                }
                Opcode::FloatGt => {
                    let reg = ins.arg(0);
                    let a = state.context.get_register(ins.arg(1));
                    let b = state.context.get_register(ins.arg(2));
                    let res = float::gt(a, b);

                    state.context.set_register(reg, res);
                }
                Opcode::FloatEq => {
                    let reg = ins.arg(0);
                    let a = state.context.get_register(ins.arg(1));
                    let b = state.context.get_register(ins.arg(2));
                    let res = float::eq(a, b);

                    state.context.set_register(reg, res);
                }
                Opcode::FloatGe => {
                    let reg = ins.arg(0);
                    let a = state.context.get_register(ins.arg(1));
                    let b = state.context.get_register(ins.arg(2));
                    let res = float::ge(a, b);

                    state.context.set_register(reg, res);
                }
                Opcode::FloatLe => {
                    let reg = ins.arg(0);
                    let a = state.context.get_register(ins.arg(1));
                    let b = state.context.get_register(ins.arg(2));
                    let res = float::le(a, b);

                    state.context.set_register(reg, res);
                }
                Opcode::ArraySet => {
                    let reg = ins.arg(0);
                    let ary = state.context.get_register(ins.arg(1));
                    let idx = state.context.get_register(ins.arg(2));
                    let val = state.context.get_register(ins.arg(3));
                    let res = array::set(ary, idx, val);

                    state.context.set_register(reg, res);
                }
                Opcode::ArrayPush => {
                    let ary = state.context.get_register(ins.arg(0));
                    let val = state.context.get_register(ins.arg(1));

                    array::push(ary, val);
                }
                Opcode::ArrayPop => {
                    let reg = ins.arg(0);
                    let ary = state.context.get_register(ins.arg(1));
                    let res = array::pop(ary);

                    state.context.set_register(reg, res);
                }
                Opcode::ArrayGet => {
                    let reg = ins.arg(0);
                    let ary = state.context.get_register(ins.arg(1));
                    let idx = state.context.get_register(ins.arg(2));
                    let res = array::get(ary, idx);

                    state.context.set_register(reg, res);
                }
                Opcode::ArrayRemove => {
                    let reg = ins.arg(0);
                    let ary = state.context.get_register(ins.arg(1));
                    let idx = state.context.get_register(ins.arg(2));
                    let res = array::remove(ary, idx);

                    state.context.set_register(reg, res);
                }
                Opcode::ArrayLength => {
                    let reg = ins.arg(0);
                    let ary = state.context.get_register(ins.arg(1));
                    let res = array::length(self.state, ary);

                    state.context.set_register(reg, res);
                }
                Opcode::StringEq => {
                    let reg = ins.arg(0);
                    let a = state.context.get_register(ins.arg(1));
                    let b = state.context.get_register(ins.arg(2));
                    let res = string::equals(a, b);

                    state.context.set_register(reg, res);
                }
                Opcode::StringSize => {
                    let reg = ins.arg(0);
                    let val = state.context.get_register(ins.arg(1));
                    let res = string::size(self.state, val);

                    state.context.set_register(reg, res);
                }
                Opcode::GetModule => {
                    let reg = ins.arg(0);
                    let id = ins.u32_arg(1, 2);
                    let res = module::get(self.state, id);

                    state.context.set_register(reg, res);
                }
                Opcode::SetField => {
                    let rec = state.context.get_register(ins.arg(0));
                    let idx = ins.arg(1);
                    let val = state.context.get_register(ins.arg(2));

                    general::set_field(rec, idx, val);
                }
                Opcode::GetField => {
                    let reg = ins.arg(0);
                    let rec = state.context.get_register(ins.arg(1));
                    let idx = ins.arg(2);
                    let res = general::get_field(rec, idx);

                    state.context.set_register(reg, res);
                }
                Opcode::ProcessAllocate => {
                    let reg = ins.arg(0);
                    let idx = ins.u32_arg(1, 2);
                    let res = process::allocate(self.state, idx);

                    state.context.set_register(reg, res);
                }
                Opcode::ProcessSend => {
                    let rec = state.context.get_register(ins.arg(0));
                    let method = ins.arg(1);
                    let wait = ins.arg(2) == 1;

                    state.save();
                    process::send_message(
                        self.state, task, process, rec, method, wait,
                    );

                    if wait {
                        return Ok(());
                    }
                }
                Opcode::ProcessSendAsync => {
                    let reg = ins.arg(0);
                    let rec = state.context.get_register(ins.arg(1));
                    let method = ins.arg(2);
                    let res = process::send_async_message(
                        self.state, task, rec, method,
                    );

                    state.context.set_register(reg, res);
                }
                Opcode::ProcessWriteResult => {
                    let reg = ins.arg(0);
                    let val = state.context.get_register(ins.arg(1));
                    let res = process::write_result(
                        self.state,
                        task,
                        val,
                        ins.arg(2) == 1,
                    );

                    state.context.set_register(reg, res);
                }
                Opcode::ProcessSuspend => {
                    let time = state.context.get_register(ins.arg(0));

                    state.save();
                    process::suspend(self.state, process, time);

                    return Ok(());
                }
                Opcode::ProcessGetField => {
                    let reg = ins.arg(0);
                    let rec = state.context.get_register(ins.arg(1));
                    let idx = ins.arg(2);
                    let res = process::get_field(rec, idx);

                    state.context.set_register(reg, res);
                }
                Opcode::ProcessSetField => {
                    let rec = state.context.get_register(ins.arg(0));
                    let idx = ins.arg(1);
                    let val = state.context.get_register(ins.arg(2));

                    process::set_field(rec, idx, val);
                }
                Opcode::ObjectEq => {
                    let reg = ins.arg(0);
                    let a = state.context.get_register(ins.arg(1));
                    let b = state.context.get_register(ins.arg(2));
                    let res = general::equals(a, b);

                    state.context.set_register(reg, res);
                }
                Opcode::GetNil => {
                    let res = Pointer::nil_singleton();

                    state.context.set_register(ins.arg(0), res);
                }
                Opcode::GetUndefined => {
                    let res = Pointer::undefined_singleton();

                    state.context.set_register(ins.arg(0), res);
                }
                Opcode::GetConstant => {
                    let reg = ins.arg(0);
                    let addr = ins.u64_arg(1, 2, 3, 4);
                    let res = Pointer::new(addr as *mut u8);

                    state.context.set_register(reg, res);
                }
                Opcode::Throw => {
                    let value = state.context.get_register(ins.arg(0));
                    let unwind = ins.arg(1) == 1;

                    process.set_throw_value(value);

                    if unwind && task.pop_context() {
                        break 'ins_loop;
                    }

                    reset!(task, state);
                }
                Opcode::MoveRegister => {
                    let reg = ins.arg(0);
                    let val = state.context.get_register(ins.arg(1));

                    state.context.set_register(reg, val);
                }
                Opcode::Panic => {
                    let msg = state.context.get_register(ins.arg(0));

                    vm_panic!(
                        unsafe { InkoString::read(&msg).to_string() },
                        state
                    );
                }
                Opcode::Exit => {
                    let status = state.context.get_register(ins.arg(0));

                    general::exit(self.state, status)?;

                    // This is just a best-case effort to clean up the current
                    // process. If it still has unfinished tasks or live
                    // objects, those are all left as-is.
                    Process::drop_and_deallocate(process);

                    return Ok(());
                }
                Opcode::StringConcat => {
                    let reg = ins.arg(0);
                    let res = string::concat(self.state, task);

                    state.context.set_register(reg, res);
                }
                Opcode::ByteArrayAllocate => {
                    let reg = ins.arg(0);
                    let res = byte_array::allocate(self.state);

                    state.context.set_register(reg, res);
                }
                Opcode::ByteArraySet => {
                    let reg = ins.arg(0);
                    let ary = state.context.get_register(ins.arg(1));
                    let idx = state.context.get_register(ins.arg(2));
                    let val = state.context.get_register(ins.arg(3));
                    let res = byte_array::set(self.state, ary, idx, val);

                    state.context.set_register(reg, res);
                }
                Opcode::ByteArrayGet => {
                    let reg = ins.arg(0);
                    let ary = state.context.get_register(ins.arg(1));
                    let idx = state.context.get_register(ins.arg(2));
                    let res = byte_array::get(ary, idx);

                    state.context.set_register(reg, res);
                }
                Opcode::ByteArrayPush => {
                    let ary = state.context.get_register(ins.arg(0));
                    let val = state.context.get_register(ins.arg(1));

                    byte_array::push(ary, val);
                }
                Opcode::ByteArrayPop => {
                    let reg = ins.arg(0);
                    let ary = state.context.get_register(ins.arg(1));
                    let res = byte_array::pop(ary);

                    state.context.set_register(reg, res);
                }
                Opcode::ByteArrayRemove => {
                    let reg = ins.arg(0);
                    let ary = state.context.get_register(ins.arg(1));
                    let idx = state.context.get_register(ins.arg(2));
                    let res = byte_array::remove(ary, idx);

                    state.context.set_register(reg, res);
                }
                Opcode::ByteArrayLength => {
                    let reg = ins.arg(0);
                    let ary = state.context.get_register(ins.arg(1));
                    let res = byte_array::length(self.state, ary);

                    state.context.set_register(reg, res);
                }
                Opcode::ByteArrayEquals => {
                    let reg = ins.arg(0);
                    let cmp = state.context.get_register(ins.arg(1));
                    let cmp_with = state.context.get_register(ins.arg(2));
                    let res = byte_array::equals(cmp, cmp_with);

                    state.context.set_register(reg, res);
                }
                Opcode::StringByte => {
                    let reg = ins.arg(0);
                    let val = state.context.get_register(ins.arg(1));
                    let idx = state.context.get_register(ins.arg(2));
                    let res = string::byte(val, idx);

                    state.context.set_register(reg, res);
                }
                Opcode::MoveResult => {
                    let reg = ins.arg(0);
                    let res = process.move_result();

                    state.context.set_register(reg, res);
                }
                Opcode::BuiltinFunctionCall => {
                    let func = ins.arg(0);

                    // When an operation would block, the file descriptor/thing
                    // is already registered, and the process may already be
                    // running again in another thread. This means that when a
                    // WouldBlock is produced it is not safe to access any
                    // process data.
                    //
                    // To ensure blocking operations are retried properly, we
                    // _first_ set the instruction index, then advance it again
                    // if it is safe to do so.
                    state.rewind();

                    match builtin_functions::call(
                        self.state, process, task, func,
                    ) {
                        Ok(val) => {
                            state.save();
                            task.clear_arguments();
                            process.set_return_value(val)
                        }
                        Err(RuntimeError::Panic(msg)) => {
                            state.save();
                            task.clear_arguments();
                            vm_panic!(msg, state);
                        }
                        Err(RuntimeError::Error(value)) => {
                            state.save();
                            task.clear_arguments();
                            process.set_throw_value(value);
                        }
                        Err(RuntimeError::WouldBlock) => {
                            // *DO NOT* use the task or process at this point,
                            // as it may have been invalidated if the process is
                            // already running again in another thread.
                            return Ok(());
                        }
                    }
                }
                Opcode::FutureGet => {
                    let fut = state.context.get_register(ins.arg(0));

                    state.rewind();

                    if future::get(self.state, process, fut) {
                        state.save();
                    } else {
                        return Ok(());
                    }
                }
                Opcode::FutureGetFor => {
                    let fut = state.context.get_register(ins.arg(0));
                    let time = state.context.get_register(ins.arg(1));

                    state.rewind();

                    if future::get_for(self.state, process, fut, time) {
                        state.save();
                    } else {
                        return Ok(());
                    }
                }
                Opcode::FuturePoll => {
                    let reg = ins.arg(0);
                    let ary = state.context.get_register(ins.arg(1));

                    state.rewind();

                    if let Some(res) = future::poll(self.state, process, ary) {
                        state.save();
                        state.context.set_register(reg, res);
                    } else {
                        return Ok(());
                    }
                }
                Opcode::CallVirtual => {
                    let rec = state.context.get_register(ins.arg(0));
                    let method = ins.arg(1);

                    state.save();
                    general::call_virtual(self.state, task, rec, method);
                    reset!(task, state);
                }
                Opcode::CallDynamic => {
                    let rec = state.context.get_register(ins.arg(0));
                    let hash = ins.u32_arg(1, 2);

                    state.save();
                    general::call_dynamic(self.state, task, rec, hash);
                    reset!(task, state);
                }
                Opcode::GetClass => {
                    let reg = ins.arg(0);
                    let id = ins.u32_arg(1, 2);
                    let res = general::get_class(self.state, id);

                    state.context.set_register(reg, res);
                }
                Opcode::RefKind => {
                    let reg = ins.arg(0);
                    let ptr = state.context.get_register(ins.arg(1));
                    let res = general::ref_kind(ptr);

                    state.context.set_register(reg, res);
                }
                Opcode::Increment => {
                    let reg = ins.arg(0);
                    let ptr = state.context.get_register(ins.arg(1));
                    let res = general::increment(ptr);

                    state.context.set_register(reg, res);
                }
                Opcode::Decrement => {
                    let ptr = state.context.get_register(ins.arg(0));

                    general::decrement(ptr);
                }
                Opcode::DecrementAtomic => {
                    let reg = ins.arg(0);
                    let ptr = state.context.get_register(ins.arg(1));
                    let res = general::decrement_atomic(ptr);

                    state.context.set_register(reg, res);
                }
                Opcode::CheckRefs => {
                    let ptr = state.context.get_register(ins.arg(0));

                    try_panic!(general::check_refs(ptr), state);
                }
                Opcode::Free => {
                    let obj = state.context.get_register(ins.arg(0));

                    general::free(obj);
                }
                Opcode::IntClone => {
                    let reg = ins.arg(0);
                    let obj = state.context.get_register(ins.arg(1));
                    let res = integer::clone(self.state, obj);

                    state.context.set_register(reg, res);
                }
                Opcode::FloatClone => {
                    let reg = ins.arg(0);
                    let obj = state.context.get_register(ins.arg(1));
                    let res = float::clone(self.state, obj);

                    state.context.set_register(reg, res);
                }
                Opcode::Reduce => {
                    reductions -= ins.arg(0) as i32;

                    // We don't need an overflow check here, as a single u16
                    // (combined with this check) can't overflow an i32.
                    if reductions <= 0 {
                        state.save();
                        self.state.scheduler.schedule(process);

                        return Ok(());
                    }
                }
                Opcode::ArrayClear => {
                    let ary = state.context.get_register(ins.arg(0));

                    array::clear(ary);
                }
                Opcode::ArrayDrop => {
                    let ary = state.context.get_register(ins.arg(0));

                    array::drop(ary);
                }
                Opcode::ByteArrayClear => {
                    let ary = state.context.get_register(ins.arg(0));

                    byte_array::clear(ary)
                }
                Opcode::ByteArrayClone => {
                    let reg = ins.arg(0);
                    let ary = state.context.get_register(ins.arg(1));
                    let res = byte_array::clone(self.state, ary);

                    state.context.set_register(reg, res);
                }
                Opcode::ByteArrayDrop => {
                    let ary = state.context.get_register(ins.arg(0));

                    byte_array::drop(ary);
                }
                Opcode::IntToFloat => {
                    let reg = ins.arg(0);
                    let val = state.context.get_register(ins.arg(1));
                    let res = integer::to_float(self.state, val);

                    state.context.set_register(reg, res);
                }
                Opcode::IntToString => {
                    let reg = ins.arg(0);
                    let val = state.context.get_register(ins.arg(1));
                    let res = integer::to_string(self.state, val);

                    state.context.set_register(reg, res);
                }
                Opcode::IntPow => {
                    let reg = ins.arg(0);
                    let a = state.context.get_register(ins.arg(1));
                    let b = state.context.get_register(ins.arg(2));
                    let res = integer::pow(self.state, a, b);

                    state.context.set_register(reg, res);
                }
                Opcode::FloatCeil => {
                    let reg = ins.arg(0);
                    let val = state.context.get_register(ins.arg(1));
                    let res = float::ceil(self.state, val);

                    state.context.set_register(reg, res);
                }
                Opcode::FloatFloor => {
                    let reg = ins.arg(0);
                    let val = state.context.get_register(ins.arg(1));
                    let res = float::floor(self.state, val);

                    state.context.set_register(reg, res);
                }
                Opcode::FloatRound => {
                    let reg = ins.arg(0);
                    let val = state.context.get_register(ins.arg(1));
                    let prec = state.context.get_register(ins.arg(2));
                    let res = float::round(self.state, val, prec);

                    state.context.set_register(reg, res);
                }
                Opcode::FloatToInt => {
                    let reg = ins.arg(0);
                    let val = state.context.get_register(ins.arg(1));
                    let res = float::to_int(self.state, val);

                    state.context.set_register(reg, res);
                }
                Opcode::FloatToString => {
                    let reg = ins.arg(0);
                    let val = state.context.get_register(ins.arg(1));
                    let res = float::to_string(self.state, val);

                    state.context.set_register(reg, res);
                }
                Opcode::FloatIsNan => {
                    let reg = ins.arg(0);
                    let val = state.context.get_register(ins.arg(1));
                    let res = float::is_nan(val);

                    state.context.set_register(reg, res);
                }
                Opcode::FloatIsInf => {
                    let reg = ins.arg(0);
                    let val = state.context.get_register(ins.arg(1));
                    let res = float::is_inf(val);

                    state.context.set_register(reg, res);
                }
                Opcode::FutureDrop => {
                    let reg = ins.arg(0);
                    let val = state.context.get_register(ins.arg(1));
                    let res = future::drop(val);

                    state.context.set_register(reg, res);
                }
                Opcode::StringDrop => {
                    let val = state.context.get_register(ins.arg(0));

                    string::drop(val);
                }
                Opcode::IsUndefined => {
                    let reg = ins.arg(0);
                    let val = state.context.get_register(ins.arg(1));
                    let res = general::is_undefined(val);

                    state.context.set_register(reg, res);
                }
                Opcode::ProcessFinishTask => {
                    let terminate = ins.arg(0) == 1;

                    if terminate {
                        if process.is_main() {
                            self.state.terminate();
                        }

                        // Processes drop/free themselves as this must be
                        // deferred until all messages (including any
                        // destructors) have finished running. If we did this in
                        // a destructor we'd end up releasing memory of a
                        // process while still using it.
                        Process::drop_and_deallocate(process);

                        return Ok(());
                    }

                    break 'ins_loop;
                }
                Opcode::JumpTable => {
                    let val = state.context.get_register(ins.arg(0));
                    let val_idx = unsafe { Int::read(val) } as usize;
                    let tbl_idx = ins.arg(1) as usize;

                    state.index =
                        state.context.method.jump_tables[tbl_idx][val_idx];
                }
                Opcode::Push => {
                    let val = state.context.get_register(ins.arg(0));

                    task.stack.push(val);
                }
                Opcode::Pop => {
                    let reg = ins.arg(0);

                    if let Some(val) = task.stack.pop() {
                        state.context.set_register(reg, val);
                    }
                }
            };
        }

        if process.finish_task() {
            self.state.scheduler.schedule(process);
        }

        Ok(())
    }

    /// Produces an Inko panic (not a Rust panic) and terminates the current
    /// program.
    ///
    /// This function is marked as cold as we expect it to be called rarely, if
    /// ever (in a correct program). This should also ensure any branches
    /// leading to this function are treated as unlikely.
    #[cold]
    #[inline(never)]
    fn panic(&self, process: ProcessPointer, message: &str) {
        let mut buffer = String::new();

        buffer.push_str("Stack trace (the most recent call comes last):");

        for location in process.stacktrace() {
            unsafe {
                let _ = write!(
                    buffer,
                    "\n  {} line {}, in '{}'",
                    InkoString::read(&location.file),
                    Int::read(location.line),
                    InkoString::read(&location.name)
                );
            }
        }

        let _ = write!(
            buffer,
            "\nProcess {:#x} panicked: {}",
            process.identifier(),
            message
        );

        eprintln!("{}", buffer);
        self.state.set_exit_status(1);
        self.state.terminate();
    }
}
