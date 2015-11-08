use compiled_code::RcCompiledCode;
use instruction::Instruction;
use object::RcObject;
use thread::RcThread;

/// Trait defining all methods that should be available for a RcVirtualMachine.
pub trait VirtualMachineMethods {
    /// Starts the main thread
    ///
    /// This requires a CompiledCode to run. Calling this method will block
    /// execution as the main thread is executed in the same OS thread as the
    /// caller of this function is operating in.
    ///
    fn start(&self, RcCompiledCode) -> Result<(), ()>;

    /// Runs a CompiledCode for a specific Thread.
    ///
    /// This iterates over all instructions in the CompiledCode, executing them
    /// one by one (except when certain instructions dictate otherwise).
    ///
    /// The return value is whatever the last CompiledCode returned (if
    /// anything). Values are only returned when a CompiledCode ends with a
    /// "return" instruction.
    ///
    fn run(&self, RcThread, RcCompiledCode)
        -> Result<Option<RcObject>, String>;

    /// Sets an integer in a register slot.
    ///
    /// This instruction requires two arguments:
    ///
    /// 1. The slot index to store the integer in.
    /// 2. The index of the integer literals to use for the value.
    ///
    /// The integer literal is extracted from the given CompiledCode.
    ///
    /// # Examples
    ///
    ///     integer_literals
    ///       0: 10
    ///
    ///     0: set_integer 0, 0
    ///
    fn ins_set_integer(&self, RcThread, RcCompiledCode, &Instruction)
        -> Result<(), String>;

    /// Sets a float in a register slot.
    ///
    /// This instruction requires two arguments:
    ///
    /// 1. The slot index to store the float in.
    /// 2. The index of the float literals to use for the value.
    ///
    /// The float literal is extracted from the given CompiledCode.
    ///
    /// # Examples
    ///
    ///     float_literals
    ///       0: 10.5
    ///
    ///     0: set_float 0, 0
    ///
    fn ins_set_float(&self, RcThread, RcCompiledCode, &Instruction)
        -> Result<(), String>;

    /// Sets a string in a register slot.
    ///
    /// This instruction requires two arguments:
    ///
    /// 1. The slot index to store the float in.
    /// 2. The index of the string literal to use for the value.
    ///
    /// The string literal is extracted from the given CompiledCode.
    ///
    /// # Examples
    ///
    ///     string_literals
    ///       0: "foo"
    ///
    ///     set_string 0, 0
    ///
    fn ins_set_string(&self, RcThread, RcCompiledCode, &Instruction)
        -> Result<(), String>;

    /// Sets an object in a register slot.
    ///
    /// This instruction requires at least one argument: the slot to store the
    /// object in. Optionally an extra argument can be provided, this argument
    /// should be a slot index pointing to the object to use as the prototype.
    ///
    /// # Examples
    ///
    ///     0: set_object 0
    ///     1: set_object 1, 0
    ///
    fn ins_set_object(&self, RcThread, RcCompiledCode, &Instruction)
        -> Result<(), String>;

    /// Sets an array in a register slot.
    ///
    /// This instruction requires at least two arguments:
    ///
    /// 1. The register slot to store the array in.
    /// 2. The amount of values to store in the array.
    ///
    /// If the 2nd argument is N where N > 0 then all N following arguments are
    /// used as values for the array.
    ///
    /// # Examples
    ///
    ///     0: set_object          0
    ///     1: set_array_prototype 0
    ///     2: set_object          1
    ///     3: set_object          2
    ///     4: set_array           3, 2, 1, 2
    ///
    fn ins_set_array(&self, RcThread, RcCompiledCode, &Instruction)
        -> Result<(), String>;

    /// Sets the name of a given object.
    ///
    /// This instruction requires two arguments:
    ///
    /// 1. The slot index containing the object to name.
    /// 2. The string literal index containing the name of the object.
    ///
    /// # Examples
    ///
    ///     string_literals
    ///       0: "Foo"
    ///
    ///     0: set_object 0
    ///     1: set_name   0, 0
    ///
    fn ins_set_name(&self, RcThread, RcCompiledCode, &Instruction)
        -> Result<(), String>;

    /// Sets the prototype for Integer objects.
    ///
    /// This instruction requires one argument: the slot index pointing to an
    /// object to use as the prototype.
    ///
    /// # Examples
    ///
    ///     0: set_object            0
    ///     1: set_integer_prototype 0
    ///
    fn ins_set_integer_prototype(&self, RcThread, RcCompiledCode, &Instruction)
        -> Result<(), String>;

    /// Sets the prototype for Float objects.
    ///
    /// This instruction requires one argument: the slot index pointing to an
    /// object to use as the prototype.
    ///
    /// # Examples
    ///
    ///     0: set_object          0
    ///     1: set_float_prototype 0
    ///
    fn ins_set_float_prototype(&self, RcThread, RcCompiledCode, &Instruction)
        -> Result<(), String>;

    /// Sets the prototype for String objects.
    ///
    /// This instruction requires one argument: the slot index pointing to an
    /// object to use as the prototype.
    ///
    /// # Examples
    ///
    ///     0: set_object           0
    ///     1: set_string_prototype 0
    ///
    fn ins_set_string_prototype(&self, RcThread, RcCompiledCode, &Instruction)
        -> Result<(), String>;

    /// Sets the prototype for Array objects.
    ///
    /// This instruction requires one argument: the slot index pointing to an
    /// object to use as the prototype.
    ///
    /// # Examples
    ///
    ///     0: set_object          0
    ///     1: set_array_prototype 0
    ///
    fn ins_set_array_prototype(&self, RcThread, RcCompiledCode, &Instruction)
        -> Result<(), String>;

    /// Sets the prototype for Thread objects.
    ///
    /// This instruction requires one argument: the slot index pointing to an
    /// object to use as the prototype.
    ///
    /// This instruction also updates any existing threads with the new
    /// prototype.
    ///
    /// # Examples
    ///
    ///     0: set_object           0
    ///     1: set_thread_prototype 0
    ///
    fn ins_set_thread_prototype(&self, RcThread, RcCompiledCode, &Instruction)
        -> Result<(), String>;

    /// Sets the prototype for "true" objects.
    ///
    /// This instruction sets the prototype used for "true" objects. This
    /// instruction requires one argument: the slot index pointing to the object
    /// to use as the prototype.
    ///
    /// # Examples
    ///
    ///     0: set_object         0
    ///     1: set_true_prototype 0
    fn ins_set_true_prototype(&self, RcThread, RcCompiledCode, &Instruction)
        -> Result<(), String>;

    /// Sets the prototype for "false" objects.
    ///
    /// This instruction sets the prototype used for "false" objects. This
    /// instruction requires one argument: the slot index pointing to the object
    /// to use as the prototype.
    ///
    /// # Examples
    ///
    ///     0: set_object          0
    ///     1: set_false_prototype 0
    fn ins_set_false_prototype(&self, RcThread, RcCompiledCode, &Instruction)
        -> Result<(), String>;

    /// Sets a "true" value in a register slot.
    ///
    /// This instruction requires only one argument: the slot index to store the
    /// object in.
    ///
    /// # Examples
    ///
    ///     0: set_object         0
    ///     1: set_true_prototype 0
    ///     2: set_true           1
    fn ins_set_true(&self, RcThread, RcCompiledCode, &Instruction)
        -> Result<(), String>;

    /// Sets a "false" value in a register slot.
    ///
    /// This instruction requires only one argument: the slot index to store the
    /// object in.
    ///
    /// # Examples
    ///
    ///     0: set_object          0
    ///     1: set_false_prototype 0
    ///     2: set_false           1
    fn ins_set_false(&self, RcThread, RcCompiledCode, &Instruction)
        -> Result<(), String>;

    /// Sets a local variable to a given slot's value.
    ///
    /// This instruction requires two arguments:
    ///
    /// 1. The local variable index to set.
    /// 2. The slot index containing the object to store in the variable.
    ///
    /// # Examples
    ///
    ///     0: set_object 0
    ///     1: set_local  0, 0
    ///
    fn ins_set_local(&self, RcThread, RcCompiledCode, &Instruction)
        -> Result<(), String>;

    /// Gets a local variable and stores it in a register slot.
    ///
    /// This instruction requires two arguments:
    ///
    /// 1. The register slot to store the local's value in.
    /// 2. The local variable index to get the value from.
    ///
    /// # Examples
    ///
    ///     0: set_object 0
    ///     1: set_local  0, 0
    ///     2: get_local  1, 0
    ///
    fn ins_get_local(&self, RcThread, RcCompiledCode, &Instruction)
        -> Result<(), String>;

    /// Sets a constant in a given object.
    ///
    /// This instruction requires 3 arguments:
    ///
    /// 1. The slot index pointing to the object to store the constant in.
    /// 2. The slot index pointing to the object to store.
    /// 3. The string literal index to use for the constant name.
    ///
    /// # Examples
    ///
    ///     string_literals
    ///       0: "Object"
    ///
    ///     0: get_toplevel 0
    ///     1: set_object   1
    ///     2: set_name     1, 0
    ///     3: set_const    0, 1, 0
    ///
    fn ins_set_const(&self, RcThread, RcCompiledCode, &Instruction)
        -> Result<(), String>;

    /// Looks up a constant and stores it in a register slot.
    ///
    /// This instruction takes 3 arguments:
    ///
    /// 1. The slot index to store the constant in.
    /// 2. The slot index pointing to an object in which to look for the
    ///    constant.
    /// 3. The string literal index containing the name of the constant.
    ///
    /// # Examples
    ///
    ///     string_literals
    ///       0: "Object"
    ///
    ///     0: get_const 0, 0
    ///
    fn ins_get_const(&self, RcThread, RcCompiledCode, &Instruction)
        -> Result<(), String>;

    /// Sets an attribute value in a specific object.
    ///
    /// This instruction requires 3 arguments:
    ///
    /// 1. The register slot containing the object for which to set the
    ///    attribute.
    /// 2. The register slot containing the object to set as the attribute
    ///    value.
    /// 3. A string literal index pointing to the name to use for the attribute.
    ///
    /// # Examples
    ///
    ///     string_literals
    ///       0: "foo"
    ///
    ///     0: set_object 0
    ///     1: set_object 1
    ///     2: set_attr   1, 0, 0
    ///
    fn ins_set_attr(&self, RcThread, RcCompiledCode, &Instruction)
        -> Result<(), String>;

    /// Gets an attribute from an object and stores it in a register slot.
    ///
    /// This instruction requires 3 arguments:
    ///
    /// 1. The register slot to store the attribute's value in.
    /// 2. The register slot containing the object from which to retrieve the
    ///    attribute.
    /// 3. A string literal index pointing to the attribute name.
    ///
    /// # Examples
    ///
    ///     string_literals
    ///       0: "foo"
    ///
    ///     0: set_object 0
    ///     1: set_object 1
    ///     2: set_attr   1, 0, 0
    ///     3: get_attr   2, 1, 0
    ///
    fn ins_get_attr(&self, RcThread, RcCompiledCode, &Instruction)
        -> Result<(), String>;

    /// Sends a message
    ///
    /// This instruction requires at least 4 arguments:
    ///
    /// 1. The slot index to store the result in.
    /// 2. The slot index of the receiver.
    /// 3. The index of the string literals to use for the method name.
    /// 4. A boolean (1 or 0) indicating if private methods can be called.
    /// 5. The amount of arguments to pass (0 or more).
    ///
    /// If the argument amount is set to N where N > 0 then the N instruction
    /// arguments following the 5th instruction argument are used as arguments
    /// for sending the message.
    ///
    /// This instruction does not allocate a String for the method name.
    ///
    /// # Examples
    ///
    ///     integer_literals
    ///       0: 10
    ///       1: 20
    ///
    ///     string_literals
    ///       0: "+"
    ///
    ///     0: set_integer 0, 0              # 10
    ///     1: set_integer 1, 1              # 20
    ///     2: send        2, 0, 0, 0, 1, 1  # 10.+(20)
    ///
    fn ins_send(&self, RcThread, RcCompiledCode, &Instruction)
        -> Result<(), String>;

    /// Returns the value in the given register slot.
    ///
    /// As register slots can be left empty this method returns an Option
    /// instead of returning an Object directly.
    ///
    /// This instruction takes a single argument: the slot index containing the
    /// value to return.
    ///
    /// # Examples
    ///
    ///     integer_literals
    ///       0: 10
    ///
    ///     0: set_integer 0, 0
    ///     1: return      0
    ///
    fn ins_return(&self, RcThread, RcCompiledCode, &Instruction)
        -> Result<Option<RcObject>, String>;

    /// Jumps to an instruction if a slot is not set or set to false.
    ///
    /// This instruction takes two arguments:
    ///
    /// 1. The instruction index to jump to if a slot is not set.
    /// 2. The slot index to check.
    ///
    /// # Examples
    ///
    ///     integer_literals
    ///       0: 10
    ///       1: 20
    ///
    ///     0: goto_if_false 0, 1
    ///     1: set_integer   0, 0
    ///     2: set_integer   0, 1
    ///
    /// Here slot "0" would be set to "20".
    fn ins_goto_if_false(&self, RcThread, RcCompiledCode, &Instruction)
        -> Result<Option<usize>, String>;

    /// Jumps to an instruction if a slot is set.
    ///
    /// This instruction takes two arguments:
    ///
    /// 1. The instruction index to jump to if a slot is set.
    /// 2. The slot index to check.
    ///
    /// # Examples
    ///
    ///     integer_literals
    ///       0: 10
    ///       1: 20
    ///
    ///     0: set_integer  0, 0
    ///     1: goto_if_true 3, 0
    ///     2: set_integer  0, 1
    ///
    /// Here slot "0" would be set to "10".
    fn ins_goto_if_true(&self, RcThread, RcCompiledCode, &Instruction)
        -> Result<Option<usize>, String>;

    /// Jumps to a specific instruction.
    ///
    /// This instruction takes one argument: the instruction index to jump to.
    ///
    /// # Examples
    ///
    ///     integer_literals:
    ///       0: 10
    ///       1: 20
    ///
    ///     0: goto        2
    ///     1: set_integer 0, 0
    ///     2: set_integer 0, 1
    ///
    /// Here slot 0 would be set to 20.
    ///
    fn ins_goto(&self, RcThread, RcCompiledCode, &Instruction)
        -> Result<usize, String>;

    /// Defines a method for an object.
    ///
    /// This instruction requires 3 arguments:
    ///
    /// 1. A slot index pointing to a specific object to define the method on.
    /// 2. The string literal index containing the method name.
    /// 3. The code object index containing the CompiledCode of the method.
    ///
    fn ins_def_method(&self, RcThread, RcCompiledCode, &Instruction)
        -> Result<(), String>;

    /// Runs a CompiledCode.
    ///
    /// This instruction takes at least 3 arguments:
    ///
    /// 1. The slot index to store the return value in.
    /// 2. The code object index pointing to the CompiledCode to run.
    /// 3. The amount of arguments to pass to the CompiledCode.
    ///
    /// If the amount of arguments is greater than 0 any following arguments are
    /// used as slot indexes for retrieving the arguments to pass to the
    /// CompiledCode.
    ///
    fn ins_run_code(&self, RcThread, RcCompiledCode, &Instruction)
        -> Result<(), String>;

    /// Sets the top-level object in a register slot.
    ///
    /// This instruction requires one argument: the slot to store the object in.
    ///
    /// # Examples
    ///
    ///     get_toplevel 0
    ///
    fn ins_get_toplevel(&self, RcThread, RcCompiledCode, &Instruction)
        -> Result<(), String>;

    /// Adds two integers
    ///
    /// This instruction requires 3 arguments:
    ///
    /// 1. The register slot to store the result in.
    /// 2. The register slot of the left-hand side object.
    /// 3. The register slot of the right-hand side object.
    ///
    /// # Examples
    ///
    ///     integer_literals:
    ///       0: 10
    ///       1: 20
    ///
    ///     0: set_integer 0, 0
    ///     1: set_integer 1, 1
    ///     2: integer_add 2, 0, 1
    ///
    fn ins_integer_add(&self, RcThread, RcCompiledCode, &Instruction)
        -> Result<(), String>;

    /// Divides an integer
    ///
    /// This instruction requires 3 arguments:
    ///
    /// 1. The register slot to store the result in.
    /// 2. The register slot of the left-hand side object.
    /// 3. The register slot of the right-hand side object.
    ///
    /// # Examples
    ///
    ///     integer_literals:
    ///       0: 10
    ///       1: 2
    ///
    ///     0: set_integer 0, 0
    ///     1: set_integer 1, 1
    ///     2: integer_div 2, 0, 1
    fn ins_integer_div(&self, RcThread, RcCompiledCode, &Instruction)
        -> Result<(), String>;

    /// Multiplies an integer
    ///
    /// This instruction requires 3 arguments:
    ///
    /// 1. The register slot to store the result in.
    /// 2. The register slot of the left-hand side object.
    /// 3. The register slot of the right-hand side object.
    ///
    /// # Examples
    ///
    ///     integer_literals:
    ///       0: 10
    ///       1: 2
    ///
    ///     0: set_integer 0, 0
    ///     1: set_integer 1, 1
    ///     2: integer_mul 2, 0, 1
    fn ins_integer_mul(&self, RcThread, RcCompiledCode, &Instruction)
        -> Result<(), String>;

    /// Subtracts an integer
    ///
    /// This instruction requires 3 arguments:
    ///
    /// 1. The register slot to store the result in.
    /// 2. The register slot of the left-hand side object.
    /// 3. The register slot of the right-hand side object.
    ///
    /// # Examples
    ///
    ///     integer_literals:
    ///       0: 10
    ///       1: 2
    ///
    ///     0: set_integer 0, 0
    ///     1: set_integer 1, 1
    ///     2: integer_sub 2, 0, 1
    fn ins_integer_sub(&self, RcThread, RcCompiledCode, &Instruction)
        -> Result<(), String>;

    /// Gets the modulo of an integer
    ///
    /// This instruction requires 3 arguments:
    ///
    /// 1. The register slot to store the result in.
    /// 2. The register slot of the left-hand side object.
    /// 3. The register slot of the right-hand side object.
    ///
    /// # Examples
    ///
    ///     integer_literals:
    ///       0: 10
    ///       1: 2
    ///
    ///     0: set_integer 0, 0
    ///     1: set_integer 1, 1
    ///     2: integer_mod 2, 0, 1
    fn ins_integer_mod(&self, RcThread, RcCompiledCode, &Instruction)
        -> Result<(), String>;

    /// Converts an integer to a float
    ///
    /// This instruction requires 2 arguments:
    ///
    /// 1. The register slot to store the result in.
    /// 2. The register slot of the integer to convert.
    ///
    /// # Examples
    ///
    ///     integer_literals:
    ///       0: 10
    ///
    ///     0: set_integer      0, 0
    ///     1: integer_to_float 1, 0
    fn ins_integer_to_float(&self, RcThread, RcCompiledCode, &Instruction)
        -> Result<(), String>;

    /// Converts an integer to a string
    ///
    /// This instruction requires 2 arguments:
    ///
    /// 1. The register slot to store the result in.
    /// 2. The register slot of the integer to convert.
    ///
    /// # Examples
    ///
    ///     integer_literals:
    ///       0: 10
    ///
    ///     0: set_integer       0, 0
    ///     1: integer_to_string 1, 0
    fn ins_integer_to_string(&self, RcThread, RcCompiledCode, &Instruction)
        -> Result<(), String>;

    /// Performs an integer bitwise AND.
    ///
    /// This instruction requires 3 arguments:
    ///
    /// 1. The register slot to store the result in.
    /// 2. The register slot of the integer to operate on.
    /// 3. The register slot of the integer to use as the operand.
    ///
    /// # Examples
    ///
    ///     integer_literals:
    ///       0: 10
    ///       1: 2
    ///
    ///     0: set_integer         0, 0
    ///     1: set_integer         1, 1
    ///     1: integer_bitwise_and 2, 0, 1
    fn ins_integer_bitwise_and(&self, RcThread, RcCompiledCode, &Instruction)
        -> Result<(), String>;

    /// Performs an integer bitwise OR.
    ///
    /// This instruction requires 3 arguments:
    ///
    /// 1. The register slot to store the result in.
    /// 2. The register slot of the integer to operate on.
    /// 3. The register slot of the integer to use as the operand.
    ///
    /// # Examples
    ///
    ///     integer_literals:
    ///       0: 10
    ///       1: 2
    ///
    ///     0: set_integer        0, 0
    ///     1: set_integer        1, 1
    ///     1: integer_bitwise_or 2, 0, 1
    fn ins_integer_bitwise_or(&self, RcThread, RcCompiledCode, &Instruction)
        -> Result<(), String>;

    /// Performs an integer bitwise XOR.
    ///
    /// This instruction requires 3 arguments:
    ///
    /// 1. The register slot to store the result in.
    /// 2. The register slot of the integer to operate on.
    /// 3. The register slot of the integer to use as the operand.
    ///
    /// # Examples
    ///
    ///     integer_literals:
    ///       0: 10
    ///       1: 2
    ///
    ///     0: set_integer         0, 0
    ///     1: set_integer         1, 1
    ///     1: integer_bitwise_xor 2, 0, 1
    fn ins_integer_bitwise_xor(&self, RcThread, RcCompiledCode, &Instruction)
        -> Result<(), String>;

    /// Shifts an integer to the left.
    ///
    /// This instruction requires 3 arguments:
    ///
    /// 1. The register slot to store the result in.
    /// 2. The register slot of the integer to operate on.
    /// 3. The register slot of the integer to use as the operand.
    ///
    /// # Examples
    ///
    ///     integer_literals:
    ///       0: 10
    ///       1: 2
    ///
    ///     0: set_integer        0, 0
    ///     1: set_integer        1, 1
    ///     1: integer_shift_left 2, 0, 1
    fn ins_integer_shift_left(&self, RcThread, RcCompiledCode, &Instruction)
        -> Result<(), String>;

    /// Shifts an integer to the right.
    ///
    /// This instruction requires 3 arguments:
    ///
    /// 1. The register slot to store the result in.
    /// 2. The register slot of the integer to operate on.
    /// 3. The register slot of the integer to use as the operand.
    ///
    /// # Examples
    ///
    ///     integer_literals:
    ///       0: 10
    ///       1: 2
    ///
    ///     0: set_integer         0, 0
    ///     1: set_integer         1, 1
    ///     1: integer_shift_right 2, 0, 1
    fn ins_integer_shift_right(&self, RcThread, RcCompiledCode, &Instruction)
        -> Result<(), String>;

    /// Checks if one integer is smaller than the other.
    ///
    /// This instruction requires 3 arguments:
    ///
    /// 1. The register slot to store the result in.
    /// 2. The register slot containing the integer to compare.
    /// 3. The register slot containing the integer to compare with.
    ///
    /// The result of this instruction is either boolean true or false.
    ///
    /// # Examples
    ///
    ///     integer_literals:
    ///       0: 10
    ///       1: 5
    ///
    ///     0: set_integer     0, 0
    ///     1: set_integer     1, 1
    ///     2: integer_smaller 2, 0, 1
    fn ins_integer_smaller(&self, RcThread, RcCompiledCode, &Instruction)
        -> Result<(), String>;

    /// Checks if one integer is greater than the other.
    ///
    /// This instruction requires 3 arguments:
    ///
    /// 1. The register slot to store the result in.
    /// 2. The register slot containing the integer to compare.
    /// 3. The register slot containing the integer to compare with.
    ///
    /// The result of this instruction is either boolean true or false.
    ///
    /// # Examples
    ///
    ///     integer_literals:
    ///       0: 10
    ///       1: 5
    ///
    ///     0: set_integer     0, 0
    ///     1: set_integer     1, 1
    ///     2: integer_greater 2, 0, 1
    fn ins_integer_greater(&self, RcThread, RcCompiledCode, &Instruction)
        -> Result<(), String>;

    /// Checks if two integers are equal.
    ///
    /// This instruction requires 3 arguments:
    ///
    /// 1. The register slot to store the result in.
    /// 2. The register slot containing the integer to compare.
    /// 3. The register slot containing the integer to compare with.
    ///
    /// The result of this instruction is either boolean true or false.
    ///
    /// # Examples
    ///
    ///     integer_literals:
    ///       0: 10
    ///       1: 5
    ///
    ///     0: set_integer   0, 0
    ///     1: set_integer   1, 1
    ///     2: integer_equal 2, 0, 1
    fn ins_integer_equal(&self, RcThread, RcCompiledCode, &Instruction)
        -> Result<(), String>;

    /// Runs a CompiledCode in a new thread.
    ///
    /// This instruction requires 2 arguments:
    ///
    /// 1. The register slot to store the thread object in.
    /// 2. A code objects index pointing to the CompiledCode object to run.
    ///
    /// # Examples
    ///
    ///     code_objects
    ///       0: CompiledCode(name="foo")
    ///
    ///     0: set_object           0
    ///     1: set_thread_prototype 0
    ///     2. start_thread         1, 0
    ///
    fn ins_start_thread(&self, RcThread, RcCompiledCode, &Instruction)
        -> Result<(), String>;

    /// Prints a VM backtrace of a given thread with a message.
    fn error(&self, RcThread, String);

    /// Runs a given CompiledCode with arguments.
    fn run_code(&self, RcThread, RcCompiledCode, Vec<RcObject>)
        -> Result<Option<RcObject>, String>;

    /// Collects a set of arguments from an instruction.
    fn collect_arguments(&self, RcThread, &Instruction, usize, usize)
        -> Result<Vec<RcObject>, String>;

    /// Runs a CompiledCode in a new thread.
    fn run_thread(&self, RcCompiledCode, bool) -> RcObject;
}
