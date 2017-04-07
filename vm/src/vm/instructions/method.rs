//! VM instruction handlers for method operations.
use binding::Binding;
use block::Block;
use object_value;
use process::RcProcess;
use vm::instruction::Instruction;
use vm::machine::Machine;

/// Looks up a method and sets it in the target register.
///
/// This instruction requires 3 arguments:
///
/// 1. The register to store the method in.
/// 2. The register containing the object containing the method.
/// 3. The register containing the method name as a String.
///
/// If a method could not be found the target register will be set to nil
/// instead.
#[inline(always)]
pub fn lookup_method(machine: &Machine,
                     process: &RcProcess,
                     instruction: &Instruction) {
    let register = instruction.arg(0);
    let rec_ptr = process.get_register(instruction.arg(1));
    let name_ptr = process.get_register(instruction.arg(2));
    let name = machine.state.intern_pointer(&name_ptr).unwrap();

    let method = rec_ptr.lookup_method(&machine.state, &name)
        .unwrap_or_else(|| machine.state.nil_object);

    process.set_register(register, method);
}

/// Defines a method for an object.
///
/// This instruction requires 3 arguments:
///
/// 1. The register to store the method object in.
/// 2. The register pointing to a specific object to define the method
///    on.
/// 3. The register containing a String to use as the method name.
/// 4. The register containing the Block to use for the method.
#[inline(always)]
pub fn def_method(machine: &Machine,
                  process: &RcProcess,
                  instruction: &Instruction) {
    let register = instruction.arg(0);
    let receiver_ptr = process.get_register(instruction.arg(1));
    let name_ptr = process.get_register(instruction.arg(2));
    let block_ptr = process.get_register(instruction.arg(3));

    if receiver_ptr.is_tagged_integer() {
        panic!("methods can not be defined on integers");
    }

    let name = machine.state.intern_pointer(&name_ptr).unwrap();
    let block = block_ptr.block_value().unwrap();

    let global_scope = block.global_scope.clone();

    let new_block = Block::new(block.code.clone(),
                               Binding::new(block.locals()),
                               global_scope);

    let value = object_value::block(new_block);
    let proto = machine.state.method_prototype;

    let method = if receiver_ptr.is_permanent() {
        machine.state
            .permanent_allocator
            .lock()
            .allocate_with_prototype(value, proto)
    } else {
        process.allocate(value, proto)
    };

    receiver_ptr.add_method(&process, name, method);

    process.set_register(register, method);
}

/// Checks if an object responds to a message.
///
/// This instruction requires 3 arguments:
///
/// 1. The register to store the result in (either true or false)
/// 2. The register containing the object to check.
/// 3. The register containing the name to look up, as a string.
#[inline(always)]
pub fn responds_to(machine: &Machine,
                   process: &RcProcess,
                   instruction: &Instruction) {
    let register = instruction.arg(0);
    let source = process.get_register(instruction.arg(1));

    let name_ptr = process.get_register(instruction.arg(2));
    let name = machine.state.intern_pointer(&name_ptr).unwrap();

    let result = if source.lookup_method(&machine.state, &name).is_some() {
        machine.state.true_object.clone()
    } else {
        machine.state.false_object.clone()
    };

    process.set_register(register, result);
}

/// Removes a method from an object.
///
/// This instruction requires 3 arguments:
///
/// 1. The register to store the removed method in.
/// 2. The register containing the object from which to remove the method.
/// 3. The register containing the method name as a string.
///
/// If the method did not exist the target register is set to nil instead.
#[inline(always)]
pub fn remove_method(machine: &Machine,
                     process: &RcProcess,
                     instruction: &Instruction) {
    let register = instruction.arg(0);
    let rec_ptr = process.get_register(instruction.arg(1));
    let name_ptr = process.get_register(instruction.arg(2));
    let name = machine.state.intern_pointer(&name_ptr).unwrap();

    if rec_ptr.is_tagged_integer() {
        panic!("methods can not be removed from integers");
    }

    let obj = if let Some(method) = rec_ptr.get_mut().remove_method(&name) {
        method
    } else {
        machine.state.nil_object
    };

    process.set_register(register, obj);
}

/// Gets all the methods available on an object.
///
/// This instruction requires 2 arguments:
///
/// 1. The register to store the methods in.
/// 2. The register containing the object for which to get all methods.
#[inline(always)]
pub fn get_methods(machine: &Machine,
                   process: &RcProcess,
                   instruction: &Instruction) {
    let register = instruction.arg(0);
    let rec_ptr = process.get_register(instruction.arg(1));
    let methods = rec_ptr.methods();

    let obj =
        process.allocate(object_value::array(methods),
                         machine.state.array_prototype);

    process.set_register(register, obj);
}

/// Gets all the method names available on an object.
///
/// This instruction requires 2 arguments:
///
/// 1. The register to store the method names in.
/// 2. The register containing the object for which to get all method names.
#[inline(always)]
pub fn get_method_names(machine: &Machine,
                        process: &RcProcess,
                        instruction: &Instruction) {
    let register = instruction.arg(0);
    let rec_ptr = process.get_register(instruction.arg(1));
    let methods = rec_ptr.method_names();

    let obj =
        process.allocate(object_value::array(methods),
                         machine.state.array_prototype);

    process.set_register(register, obj);
}
