//! VM instruction handlers for process operations.
use object_pointer::ObjectPointer;
use pools::PRIMARY_POOL;
use process::RcProcess;
use vm::instruction::Instruction;
use vm::machine::Machine;

/// Spawns a new process.
///
/// This instruction takes 3 arguments:
///
/// 1. The register to store the PID in.
/// 2. The register containing the Block to run in the process.
/// 3. The register containing the ID of the process pool to schedule the
///    process on. Defaults to the ID of the primary pool.
#[inline(always)]
pub fn spawn_process(machine: &Machine,
                     process: &RcProcess,
                     instruction: &Instruction) {
    let register = instruction.arg(0);
    let block_ptr = process.get_register(instruction.arg(1));

    let pool_id = if let Some(pool_reg) = instruction.arg_opt(2) {
        let ptr = process.get_register(pool_reg);

        ptr.integer_value().unwrap() as usize
    } else {
        PRIMARY_POOL
    };

    let block_obj = block_ptr.block_value().unwrap();
    let new_proc = machine.allocate_process(pool_id, block_obj).unwrap();
    let new_pid = new_proc.pid;

    machine.state.process_pools.schedule(new_proc);

    process.set_register(register, ObjectPointer::integer(new_pid as i64));
}

/// Sends a message to a process.
///
/// This instruction takes 3 arguments:
///
/// 1. The register to store the message in.
/// 2. The register containing the PID to send the message to.
/// 3. The register containing the message (an object) to send to the
///    process.
#[inline(always)]
pub fn send_process_message(machine: &Machine,
                            process: &RcProcess,
                            instruction: &Instruction) {
    let register = instruction.arg(0);
    let pid_ptr = process.get_register(instruction.arg(1));
    let msg_ptr = process.get_register(instruction.arg(2));
    let pid = pid_ptr.integer_value().unwrap() as usize;

    if let Some(receiver) = read_lock!(machine.state.process_table).get(&pid) {
        receiver.send_message(&process, msg_ptr);
    }

    process.set_register(register, msg_ptr);
}

/// Receives a message for the current process.
///
/// This instruction takes 1 argument: the register to store the resulting
/// message in.
///
/// If no messages are available the current process will be suspended, and
/// the instruction will be retried the next time the process is executed.
#[inline(always)]
pub fn receive_process_message(process: &RcProcess,
                               instruction: &Instruction)
                               -> bool {
    if let Some(msg_ptr) = process.receive_message() {
        process.set_register(instruction.arg(0), msg_ptr);

        false
    } else {
        true
    }
}

/// Gets the PID of the currently running process.
///
/// This instruction requires one argument: the register to store the PID
/// in (as an integer).
#[inline(always)]
pub fn get_current_pid(process: &RcProcess, instruction: &Instruction) {
    let register = instruction.arg(0);
    let pid = process.pid;

    process.set_register(register, ObjectPointer::integer(pid as i64));
}
