//! VM instruction handlers for regular object operations.
use immix::copy_object::CopyObject;
use object_value;
use process::RcProcess;
use vm::instruction::Instruction;
use vm::machine::Machine;

/// Sets an object in a register.
///
/// This instruction takes 3 arguments:
///
/// 1. The register to store the object in.
/// 2. A register containing a truthy/falsy object. When the register
///    contains a truthy object the new object will be a global object.
/// 3. An optional register containing the prototype for the object.
#[inline(always)]
pub fn set_object(machine: &Machine,
                  process: &RcProcess,
                  instruction: &Instruction) {
    let register = instruction.arg(0);
    let is_permanent_ptr = process.get_register(instruction.arg(1));
    let is_permanent = is_permanent_ptr != machine.state.false_object;

    let obj = if is_permanent {
        machine.state.permanent_allocator.lock().allocate_empty()
    } else {
        process.allocate_empty()
    };

    if let Some(proto_index) = instruction.arg_opt(2) {
        let mut proto = process.get_register(proto_index);

        if is_permanent && !proto.is_permanent() {
            proto = machine.state
                .permanent_allocator
                .lock()
                .copy_object(proto);
        }

        obj.get_mut().set_prototype(proto);
    }

    process.set_register(register, obj);
}

/// Sets an attribute of an object.
///
/// This instruction requires 3 arguments:
///
/// 1. The register containing the object for which to set the
///    attribute.
/// 2. The register containing the attribute name as a string.
/// 3. The register containing the object to set as the value.
#[inline(always)]
pub fn set_attr(machine: &Machine,
                process: &RcProcess,
                instruction: &Instruction) {
    let target_ptr = process.get_register(instruction.arg(0));
    let name_ptr = process.get_register(instruction.arg(1));
    let value_ptr = process.get_register(instruction.arg(2));

    if target_ptr.is_tagged_integer() {
        panic!("attributes can not be set for integers");
    }

    let name = machine.state.intern_pointer(&name_ptr).unwrap();

    let value = copy_if_permanent!(machine.state.permanent_allocator,
                                   value_ptr,
                                   target_ptr);

    target_ptr.add_attribute(&process, name.clone(), value);
}

/// Gets an attribute from an object and stores it in a register.
///
/// This instruction requires 3 arguments:
///
/// 1. The register to store the attribute's value in.
/// 2. The register containing the object from which to retrieve the
///    attribute.
/// 3. The register containing the attribute name as a string.
///
/// If the attribute does not exist the target register is set to nil.
#[inline(always)]
pub fn get_attr(machine: &Machine,
                process: &RcProcess,
                instruction: &Instruction) {
    let register = instruction.arg(0);
    let source = process.get_register(instruction.arg(1));
    let name_ptr = process.get_register(instruction.arg(2));
    let name = machine.state.intern_pointer(&name_ptr).unwrap();

    let attr = source.lookup_attribute(&name)
        .unwrap_or_else(|| machine.state.nil_object);

    process.set_register(register, attr);
}

/// Checks if an attribute exists in an object.
///
/// This instruction requires 3 arguments:
///
/// 1. The register to store the result in (true or false).
/// 2. The register containing the object to check.
/// 3. The register containing the attribute name as a string.
#[inline(always)]
pub fn attr_exists(machine: &Machine,
                   process: &RcProcess,
                   instruction: &Instruction) {
    let register = instruction.arg(0);
    let source_ptr = process.get_register(instruction.arg(1));
    let name_ptr = process.get_register(instruction.arg(2));

    let name = machine.state.intern_pointer(&name_ptr).unwrap();

    let obj = if source_ptr.lookup_attribute(&name).is_some() {
        machine.state.true_object.clone()
    } else {
        machine.state.false_object.clone()
    };

    process.set_register(register, obj);
}

/// Checks if two objects are equal.
///
/// Comparing equality is done by simply comparing the addresses of both
/// pointers: if they're equal then the objects are also considered to be equal.
///
/// This instruction takes 3 arguments:
///
/// 1. The register to store the result in.
/// 2. The register containing the object to compare.
/// 3. The register containing the object to compare with.
///
/// The result of this instruction is either boolean true, or false.
#[inline(always)]
pub fn object_equals(machine: &Machine,
                     process: &RcProcess,
                     instruction: &Instruction) {
    let register = instruction.arg(0);
    let compare = process.get_register(instruction.arg(1));
    let compare_with = process.get_register(instruction.arg(2));

    let obj = if compare == compare_with {
        machine.state.true_object
    } else {
        machine.state.false_object
    };

    process.set_register(register, obj);
}

/// Sets the top-level object in a register.
///
/// This instruction requires one argument: the register to store the object
/// in.
#[inline(always)]
pub fn get_toplevel(machine: &Machine,
                    process: &RcProcess,
                    instruction: &Instruction) {
    let register = instruction.arg(0);

    process.set_register(register, machine.state.top_level);
}

/// Removes a attribute from an object.
///
/// This instruction requires 3 arguments:
///
/// 1. The register to store the removed attribute in.
/// 2. The register containing the object from which to remove the attribute.
/// 3. The register containing the attribute name as a string.
///
/// If the attribute did not exist the target register is set to nil instead.
#[inline(always)]
pub fn remove_attribute(machine: &Machine,
                        process: &RcProcess,
                        instruction: &Instruction) {
    let register = instruction.arg(0);
    let rec_ptr = process.get_register(instruction.arg(1));
    let name_ptr = process.get_register(instruction.arg(2));
    let name = machine.state.intern_pointer(&name_ptr).unwrap();

    if rec_ptr.is_tagged_integer() {
        panic!("attributes can not be removed for integers");
    }

    let obj = if let Some(attribute) = rec_ptr.get_mut()
        .remove_attribute(&name) {
        attribute
    } else {
        machine.state.nil_object
    };

    process.set_register(register, obj);
}

/// Gets all the attributes available on an object.
///
/// This instruction requires 2 arguments:
///
/// 1. The register to store the attributes in.
/// 2. The register containing the object for which to get all attributes.
#[inline(always)]
pub fn get_attributes(machine: &Machine,
                      process: &RcProcess,
                      instruction: &Instruction) {
    let register = instruction.arg(0);
    let rec_ptr = process.get_register(instruction.arg(1));
    let attributes = rec_ptr.attributes();

    let obj = process.allocate(object_value::array(attributes),
                               machine.state.array_prototype);

    process.set_register(register, obj);
}

/// Gets all the attributes names available on an object.
///
/// This instruction requires 2 arguments:
///
/// 1. The register to store the attribute names in.
/// 2. The register containing the object for which to get all attributes names.
#[inline(always)]
pub fn get_attribute_names(machine: &Machine,
                           process: &RcProcess,
                           instruction: &Instruction) {
    let register = instruction.arg(0);
    let rec_ptr = process.get_register(instruction.arg(1));
    let attributes = rec_ptr.attribute_names();

    let obj = process.allocate(object_value::array(attributes),
                               machine.state.array_prototype);

    process.set_register(register, obj);
}
