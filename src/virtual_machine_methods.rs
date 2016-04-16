use binding::RcBinding;
use compiled_code::RcCompiledCode;
use instruction::Instruction;
use object::RcObject;
use thread::RcThread;
use virtual_machine_result::*;

/// Trait defining all methods that should be available for a RcVirtualMachine.
pub trait VirtualMachineMethods {
    /// Starts the main thread
    ///
    /// This requires a RcCompiledCode to run. Calling this method will block
    /// execution as the main thread is executed in the same OS thread as the
    /// caller of this function is operating in.
    fn start(&self, RcCompiledCode) -> Result<(), ()>;

    /// Runs a CompiledCode for a specific Thread.
    ///
    /// This iterates over all instructions in the CompiledCode, executing them
    /// one by one (except when certain instructions dictate otherwise).
    ///
    /// The return value is whatever the last CompiledCode returned (if
    /// anything). Values are only returned when a CompiledCode ends with a
    /// "return" instruction.
    fn run(&self, RcThread, RcCompiledCode) -> OptionObjectResult;

    /// Sets an integer in a register.
    ///
    /// This instruction requires two arguments:
    ///
    /// 1. The register to store the integer in.
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
    fn ins_set_integer(&self, RcThread, RcCompiledCode, &Instruction)
        -> EmptyResult;

    /// Sets a float in a register.
    ///
    /// This instruction requires two arguments:
    ///
    /// 1. The register to store the float in.
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
    fn ins_set_float(&self, RcThread, RcCompiledCode, &Instruction)
        -> EmptyResult;

    /// Sets a string in a register.
    ///
    /// This instruction requires two arguments:
    ///
    /// 1. The register to store the float in.
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
    fn ins_set_string(&self, RcThread, RcCompiledCode, &Instruction)
        -> EmptyResult;

    /// Sets an object in a register.
    ///
    /// This instruction requires at least one argument: the register to store
    /// the object in. Optionally an extra argument can be provided, this
    /// argument should be a register pointing to the object to use as the
    /// prototype.
    ///
    /// # Examples
    ///
    ///     0: set_object 0
    ///     1: set_object 1, 0
    fn ins_set_object(&self, RcThread, RcCompiledCode, &Instruction)
        -> EmptyResult;

    /// Sets an array in a register.
    ///
    /// This instruction requires at least two arguments:
    ///
    /// 1. The register to store the array in.
    /// 2. The amount of values to store in the array.
    ///
    /// If the 2nd argument is N where N > 0 then all N following arguments are
    /// used as values for the array.
    ///
    /// # Examples
    ///
    ///     0: set_object 0
    ///     2: set_object 1
    ///     3: set_object 2
    ///     4: set_array  3, 2, 1, 2
    fn ins_set_array(&self, RcThread, RcCompiledCode, &Instruction)
        -> EmptyResult;

    /// Sets the name of a given object.
    ///
    /// This instruction requires two arguments:
    ///
    /// 1. The register containing the object to name.
    /// 2. The string literal index containing the name of the object.
    ///
    /// # Examples
    ///
    ///     string_literals
    ///       0: "Foo"
    ///
    ///     0: set_object 0
    ///     1: set_name   0, 0
    fn ins_set_name(&self, RcThread, RcCompiledCode, &Instruction)
        -> EmptyResult;

    /// Returns the prototype to use for integer objects.
    ///
    /// This instruction requires one argument: the register to store the
    /// prototype in.
    ///
    /// # Examples
    ///
    ///     0: get_integer_prototype 0
    fn ins_get_integer_prototype(&self, RcThread, RcCompiledCode, &Instruction)
        -> EmptyResult;

    /// Returns the prototype to use for float objects.
    ///
    /// This instruction requires one argument: the register to store the
    /// prototype in.
    ///
    /// # Examples
    ///
    ///     0: get_float_prototype 0
    fn ins_get_float_prototype(&self, RcThread, RcCompiledCode, &Instruction)
        -> EmptyResult;

    /// Returns the prototype to use for string objects.
    ///
    /// This instruction requires one argument: the register to store the
    /// prototype in.
    ///
    /// # Examples
    ///
    ///     0: get_string_prototype 0
    fn ins_get_string_prototype(&self, RcThread, RcCompiledCode, &Instruction)
        -> EmptyResult;

    /// Returns the prototype to use for array objects.
    ///
    /// This instruction requires one argument: the register to store the
    /// prototype in.
    ///
    /// # Examples
    ///
    ///     0: get_array_prototype 0
    fn ins_get_array_prototype(&self, RcThread, RcCompiledCode, &Instruction)
        -> EmptyResult;

    /// Gets the prototype to use for thread objects.
    ///
    /// This instruction requires one argument: the register to store the
    /// prototype in.
    ///
    /// # Examples
    ///
    ///     0: get_thread_prototype 0
    fn ins_get_thread_prototype(&self, RcThread, RcCompiledCode, &Instruction)
        -> EmptyResult;

    /// Gets the prototype to use for true objects.
    ///
    /// This instruction requires one argument: the register to store the
    /// prototype in.
    ///
    /// # Examples
    ///
    ///     0: get_true_prototype 0
    fn ins_get_true_prototype(&self, RcThread, RcCompiledCode, &Instruction)
        -> EmptyResult;

    /// Gets the prototype to use for false objects.
    ///
    /// This instruction requires one argument: the register to store the
    /// prototype in.
    ///
    /// # Examples
    ///
    ///     0: get_false_prototype 0
    fn ins_get_false_prototype(&self, RcThread, RcCompiledCode, &Instruction)
        -> EmptyResult;

    /// Gets the prototype to use for method objects.
    ///
    /// This instruction requires one argument: the register to store the
    /// prototype in.
    ///
    /// # Examples
    ///
    ///     0: get_method_prototype 0
    fn ins_get_method_prototype(&self, RcThread, RcCompiledCode, &Instruction)
        -> EmptyResult;

    /// Gets the prototype to use for Binding objects.
    ///
    /// This instruction requires one argument: the register to store the
    /// prototype in.
    ///
    /// # Examples
    ///
    ///     0: get_binding_prototype 0
    fn ins_get_binding_prototype(&self, RcThread, RcCompiledCode, &Instruction)
        -> EmptyResult;

    /// Gets the prototype to use for compiled code objects.
    ///
    /// This instruction requires one argument: the register to store the
    /// prototype in.
    ///
    /// # Examples
    ///
    ///     0: get_compiled_code_prototype 0
    fn ins_get_compiled_code_prototype(&self, RcThread, RcCompiledCode,
                                       &Instruction) -> EmptyResult;

    /// Sets a "true" value in a register.
    ///
    /// This instruction requires only one argument: the register to store the
    /// object in.
    ///
    /// # Examples
    ///
    ///     0: get_true 1
    fn ins_get_true(&self, RcThread, RcCompiledCode, &Instruction)
        -> EmptyResult;

    /// Sets a "false" value in a register.
    ///
    /// This instruction requires only one argument: the register to store the
    /// object in.
    ///
    /// # Examples
    ///
    ///     0: get_false 1
    fn ins_get_false(&self, RcThread, RcCompiledCode, &Instruction)
        -> EmptyResult;

    /// Gets the Binding of the current scope and sets it in a register
    ///
    /// This instruction requires only one argument: the register to store the
    /// object in.
    ///
    /// # Examples
    ///
    ///     0: get_binding 0
    fn ins_get_binding(&self, RcThread, RcCompiledCode, &Instruction)
        -> EmptyResult;

    /// Sets a local variable to a given register's value.
    ///
    /// This instruction requires two arguments:
    ///
    /// 1. The local variable index to set.
    /// 2. The register containing the object to store in the variable.
    ///
    /// # Examples
    ///
    ///     0: set_object 0
    ///     1: set_local  0, 0
    fn ins_set_local(&self, RcThread, RcCompiledCode, &Instruction)
        -> EmptyResult;

    /// Gets a local variable and stores it in a register.
    ///
    /// This instruction requires two arguments:
    ///
    /// 1. The register to store the local's value in.
    /// 2. The local variable index to get the value from.
    ///
    /// # Examples
    ///
    ///     0: set_object 0
    ///     1: set_local  0, 0
    ///     2: get_local  1, 0
    fn ins_get_local(&self, RcThread, RcCompiledCode, &Instruction)
        -> EmptyResult;

    /// Sets a constant in a given object.
    ///
    /// This instruction requires 3 arguments:
    ///
    /// 1. The register pointing to the object to store the constant in.
    /// 2. The register pointing to the object to store.
    /// 3. The register containing .
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
    fn ins_set_const(&self, RcThread, RcCompiledCode, &Instruction)
        -> EmptyResult;

    /// Looks up a constant and stores it in a register.
    ///
    /// This instruction takes 3 arguments:
    ///
    /// 1. The register to store the constant in.
    /// 2. The register pointing to an object in which to look for the
    ///    constant.
    /// 3. The string literal index containing the name of the constant.
    ///
    /// # Examples
    ///
    ///     string_literals
    ///       0: "Object"
    ///
    ///     0: get_const 0, 0
    fn ins_get_const(&self, RcThread, RcCompiledCode, &Instruction)
        -> EmptyResult;

    /// Sets an attribute value in a specific object.
    ///
    /// This instruction requires 3 arguments:
    ///
    /// 1. The register containing the object for which to set the
    ///    attribute.
    /// 2. The register containing the object to set as the attribute
    ///    value.
    /// 3. The register containing a String to use for the attribute name.
    ///
    /// # Examples
    ///
    ///     string_literals
    ///       0: "foo"
    ///
    ///     0: set_object 0
    ///     1: set_object 1
    ///     2: set_string 2, 0
    ///     3: set_attr   3, 0, 2
    fn ins_set_attr(&self, RcThread, RcCompiledCode, &Instruction)
        -> EmptyResult;

    /// Gets an attribute from an object and stores it in a register.
    ///
    /// This instruction requires 3 arguments:
    ///
    /// 1. The register to store the attribute's value in.
    /// 2. The register containing the object from which to retrieve the
    ///    attribute.
    /// 3. The register containing a String to use for the attribute name.
    ///
    /// # Examples
    ///
    ///     string_literals
    ///       0: "foo"
    ///
    ///     0: set_object 0
    ///     1: set_object 1
    ///     2: set_string 2, 0
    ///     3: set_attr   0, 1, 2
    ///     4: get_attr   3, 0, 2
    fn ins_get_attr(&self, RcThread, RcCompiledCode, &Instruction)
        -> EmptyResult;

    /// Sets a CompiledCode object in a register.
    ///
    /// This instruction requires two arguments:
    ///
    /// 1. The register to store the object in.
    /// 2. The index of the compiled code object to store.
    ///
    /// # Examples:
    ///
    ///     code_objects:
    ///       0: ...
    ///
    ///     0: set_compiled_code 0, 0
    fn ins_set_compiled_code(&self, RcThread, RcCompiledCode, &Instruction)
        -> EmptyResult;

    /// Sends a message using a string literal
    ///
    /// This instruction requires at least 4 arguments:
    ///
    /// 1. The register to store the result in.
    /// 2. The register of the receiver.
    /// 3. The index of the string literal to use for the method name.
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
    fn ins_send_literal(&self, RcThread, RcCompiledCode, &Instruction)
        -> EmptyResult;

    /// Sends a message using a runtime allocated string
    ///
    /// This instruction takes the same arguments as the "send_literal"
    /// instruction except instead of the 3rd argument pointing to a string
    /// literal it should point to a register containing a string.
    fn ins_send(&self, RcThread, RcCompiledCode, &Instruction) -> EmptyResult;

    /// Returns the value in the given register.
    ///
    /// As registers can be left empty this method returns an Option
    /// instead of returning an Object directly.
    ///
    /// This instruction takes a single argument: the register containing the
    /// value to return.
    ///
    /// # Examples
    ///
    ///     integer_literals
    ///       0: 10
    ///
    ///     0: set_integer 0, 0
    ///     1: return      0
    fn ins_return(&self, RcThread, RcCompiledCode, &Instruction)
        -> OptionObjectResult;

    /// Jumps to an instruction if a register is not set or set to false.
    ///
    /// This instruction takes two arguments:
    ///
    /// 1. The instruction index to jump to if a register is not set.
    /// 2. The register to check.
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
    /// Here register "0" would be set to "20".
    fn ins_goto_if_false(&self, RcThread, RcCompiledCode, &Instruction)
        -> OptionIntegerResult;

    /// Jumps to an instruction if a register is set.
    ///
    /// This instruction takes two arguments:
    ///
    /// 1. The instruction index to jump to if a register is set.
    /// 2. The register to check.
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
    /// Here register "0" would be set to "10".
    fn ins_goto_if_true(&self, RcThread, RcCompiledCode, &Instruction)
        -> OptionIntegerResult;

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
    /// Here register 0 would be set to 20.
    fn ins_goto(&self, RcThread, RcCompiledCode, &Instruction) -> IntegerResult;

    /// Defines a method for an object.
    ///
    /// This instruction requires 3 arguments:
    ///
    /// 1. The register pointing to a specific object to define the method
    ///    on.
    /// 2. The register containing a String to use as the method name.
    /// 3. The register containing the CompiledCode object to use for the
    ///    method.
    fn ins_def_method(&self, RcThread, RcCompiledCode, &Instruction)
        -> EmptyResult;

    /// Defines a method for an object using literals.
    ///
    /// This instruction can be used to define a method when the name and the
    /// compiled code object are defined as literals. This instruction is
    /// primarily meant to define methods that are defined directly in the
    /// source code. Methods defined during runtime should be created using the
    /// `def_method` instruction instead.
    ///
    /// This instruction requires 3 arguments:
    ///
    /// 1. The register pointing to the object to define the method on.
    /// 2. The string literal index to use for the method name.
    /// 3. The code object index to use for the method's CompiledCode object.
    fn ins_def_literal_method(&self, RcThread, RcCompiledCode, &Instruction)
        -> EmptyResult;

    /// Runs a CompiledCode.
    ///
    /// This instruction takes the following arguments:
    ///
    /// 1. The register to store the return value in.
    /// 2. The register containing the CompiledCode object to run.
    /// 3. The register containing the amount of arguments to pass.
    /// 4. The arguments to pass when the argument count is greater than 0, each
    ///    as a separate argument.
    /// 5. The Binding to use, if any. Omitting this argument results in a
    ///    Binding being created automatically.
    fn ins_run_code(&self, RcThread, RcCompiledCode, &Instruction)
        -> EmptyResult;

    /// Sets the top-level object in a register.
    ///
    /// This instruction requires one argument: the register to store the object
    /// in.
    ///
    /// # Examples
    ///
    ///     get_toplevel 0
    fn ins_get_toplevel(&self, RcThread, RcCompiledCode, &Instruction)
        -> EmptyResult;

    /// Sets the object "self" refers to in a register.
    ///
    /// This instruction requires one argument: the register to store the object
    /// in.
    ///
    /// # Examples
    ///
    ///     get_self 0
    fn ins_get_self(&self, RcThread, RcCompiledCode, &Instruction)
        -> EmptyResult;

    /// Checks if a given object is an error object.
    ///
    /// This instruction requires two arguments:
    ///
    /// 1. The register to store the boolean result in.
    /// 2. The register of the object to check.
    ///
    /// # Examples
    ///
    ///     0: stdin_read 0
    ///     1: is_error   1, 0
    fn ins_is_error(&self, RcThread, RcCompiledCode, &Instruction)
        -> EmptyResult;

    /// Converts an error object to an integer.
    ///
    /// This instruction requires two arguments:
    ///
    /// 1. The register to store the integer in.
    /// 2. The register containing the error.
    ///
    /// # Examples
    ///
    ///     0: stdin_read       0
    ///     1: is_error         1, 0
    ///     2: goto_if_false    4, 1
    ///     3: error_to_integer 2, 0
    fn ins_error_to_integer(&self, RcThread, RcCompiledCode, &Instruction)
        -> EmptyResult;

    /// Adds two integers
    ///
    /// This instruction requires 3 arguments:
    ///
    /// 1. The register to store the result in.
    /// 2. The register of the left-hand side object.
    /// 3. The register of the right-hand side object.
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
    fn ins_integer_add(&self, RcThread, RcCompiledCode, &Instruction)
        -> EmptyResult;

    /// Divides an integer
    ///
    /// This instruction requires 3 arguments:
    ///
    /// 1. The register to store the result in.
    /// 2. The register of the left-hand side object.
    /// 3. The register of the right-hand side object.
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
        -> EmptyResult;

    /// Multiplies an integer
    ///
    /// This instruction requires 3 arguments:
    ///
    /// 1. The register to store the result in.
    /// 2. The register of the left-hand side object.
    /// 3. The register of the right-hand side object.
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
        -> EmptyResult;

    /// Subtracts an integer
    ///
    /// This instruction requires 3 arguments:
    ///
    /// 1. The register to store the result in.
    /// 2. The register of the left-hand side object.
    /// 3. The register of the right-hand side object.
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
        -> EmptyResult;

    /// Gets the modulo of an integer
    ///
    /// This instruction requires 3 arguments:
    ///
    /// 1. The register to store the result in.
    /// 2. The register of the left-hand side object.
    /// 3. The register of the right-hand side object.
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
        -> EmptyResult;

    /// Converts an integer to a float
    ///
    /// This instruction requires 2 arguments:
    ///
    /// 1. The register to store the result in.
    /// 2. The register of the integer to convert.
    ///
    /// # Examples
    ///
    ///     integer_literals:
    ///       0: 10
    ///
    ///     0: set_integer      0, 0
    ///     1: integer_to_float 1, 0
    fn ins_integer_to_float(&self, RcThread, RcCompiledCode, &Instruction)
        -> EmptyResult;

    /// Converts an integer to a string
    ///
    /// This instruction requires 2 arguments:
    ///
    /// 1. The register to store the result in.
    /// 2. The register of the integer to convert.
    ///
    /// # Examples
    ///
    ///     integer_literals:
    ///       0: 10
    ///
    ///     0: set_integer       0, 0
    ///     1: integer_to_string 1, 0
    fn ins_integer_to_string(&self, RcThread, RcCompiledCode, &Instruction)
        -> EmptyResult;

    /// Performs an integer bitwise AND.
    ///
    /// This instruction requires 3 arguments:
    ///
    /// 1. The register to store the result in.
    /// 2. The register of the integer to operate on.
    /// 3. The register of the integer to use as the operand.
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
        -> EmptyResult;

    /// Performs an integer bitwise OR.
    ///
    /// This instruction requires 3 arguments:
    ///
    /// 1. The register to store the result in.
    /// 2. The register of the integer to operate on.
    /// 3. The register of the integer to use as the operand.
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
        -> EmptyResult;

    /// Performs an integer bitwise XOR.
    ///
    /// This instruction requires 3 arguments:
    ///
    /// 1. The register to store the result in.
    /// 2. The register of the integer to operate on.
    /// 3. The register of the integer to use as the operand.
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
        -> EmptyResult;

    /// Shifts an integer to the left.
    ///
    /// This instruction requires 3 arguments:
    ///
    /// 1. The register to store the result in.
    /// 2. The register of the integer to operate on.
    /// 3. The register of the integer to use as the operand.
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
        -> EmptyResult;

    /// Shifts an integer to the right.
    ///
    /// This instruction requires 3 arguments:
    ///
    /// 1. The register to store the result in.
    /// 2. The register of the integer to operate on.
    /// 3. The register of the integer to use as the operand.
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
        -> EmptyResult;

    /// Checks if one integer is smaller than the other.
    ///
    /// This instruction requires 3 arguments:
    ///
    /// 1. The register to store the result in.
    /// 2. The register containing the integer to compare.
    /// 3. The register containing the integer to compare with.
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
        -> EmptyResult;

    /// Checks if one integer is greater than the other.
    ///
    /// This instruction requires 3 arguments:
    ///
    /// 1. The register to store the result in.
    /// 2. The register containing the integer to compare.
    /// 3. The register containing the integer to compare with.
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
        -> EmptyResult;

    /// Checks if two integers are equal.
    ///
    /// This instruction requires 3 arguments:
    ///
    /// 1. The register to store the result in.
    /// 2. The register containing the integer to compare.
    /// 3. The register containing the integer to compare with.
    ///
    /// The result of this instruction is either boolean true or false.
    ///
    /// # Examples
    ///
    ///     integer_literals:
    ///       0: 10
    ///       1: 5
    ///
    ///     0: set_integer    0, 0
    ///     1: set_integer    1, 1
    ///     2: integer_equals 2, 0, 1
    fn ins_integer_equals(&self, RcThread, RcCompiledCode, &Instruction)
        -> EmptyResult;

    /// Runs a CompiledCode in a new thread.
    ///
    /// This instruction requires 2 arguments:
    ///
    /// 1. The register to store the thread object in.
    /// 2. A code objects index pointing to the CompiledCode object to run.
    ///
    /// # Examples
    ///
    ///     code_objects
    ///       0: CompiledCode(name="foo")
    ///
    ///     0: set_object   0
    ///     2. start_thread 1, 0
    fn ins_start_thread(&self, RcThread, RcCompiledCode, &Instruction)
        -> EmptyResult;

    /// Adds two floats
    ///
    /// This instruction requires 3 arguments:
    ///
    /// 1. The register to store the result in.
    /// 2. The register of the receiver.
    /// 3. The register of the float to add.
    ///
    /// # Examples
    ///
    ///     float_literals:
    ///       0: 10.5
    ///       1: 0.5
    ///
    ///     0: set_float 0, 0
    ///     1: set_float 1, 1
    ///     2: float_add 2, 0, 1
    fn ins_float_add(&self, RcThread, RcCompiledCode, &Instruction)
        -> EmptyResult;

    /// Multiplies two floats
    ///
    /// This instruction requires 3 arguments:
    ///
    /// 1. The register to store the result in.
    /// 2. The register of the receiver.
    /// 3. The register of the float to multiply with.
    ///
    /// # Examples
    ///
    ///     float_literals:
    ///       0: 10.5
    ///       1. 2.0
    ///
    ///     0: set_float 0, 0
    ///     1: set_float 1, 1
    ///     3: float_mul 2, 0, 1
    fn ins_float_mul(&self, RcThread, RcCompiledCode, &Instruction)
        -> EmptyResult;

    /// Divides two floats
    ///
    /// This instruction requires 3 arguments:
    ///
    /// 1. The register to store the result in.
    /// 2. The register of the receiver.
    /// 3. The register of the float to divide with.
    ///
    /// # Examples
    ///
    ///     float_literals:
    ///       0: 10.5
    ///       1: 2.0
    ///
    ///     0: set_float 0, 0
    ///     1: set_float 1, 1
    ///     2: float_div 2, 0, 1
    fn ins_float_div(&self, RcThread, RcCompiledCode, &Instruction)
        -> EmptyResult;

    /// Subtracts two floats
    ///
    /// This instruction requires 3 arguments:
    ///
    /// 1. The register to store the result in.
    /// 2. The register of the receiver.
    /// 3. The register of the float to subtract.
    ///
    /// # Examples
    ///
    ///     float_literals:
    ///       0: 10.5
    ///       1: 5.0
    ///
    ///     0: set_float 0, 0
    ///     1: set_float 1, 1
    ///     2: float_sub 2, 0, 1
    fn ins_float_sub(&self, RcThread, RcCompiledCode, &Instruction)
        -> EmptyResult;

    /// Gets the modulo of a float
    ///
    /// This instruction requires 3 arguments:
    ///
    /// 1. The register to store the result in.
    /// 2. The register of the receiver.
    /// 3. The register of the float argument.
    ///
    /// # Examples
    ///
    ///     float_literals:
    ///       0: 10.5
    ///       1: 5.0
    ///
    ///     0: set_float 0, 0
    ///     1: set_float 1, 1
    ///     2: float_mod 2, 0, 1
    fn ins_float_mod(&self, RcThread, RcCompiledCode, &Instruction)
        -> EmptyResult;

    /// Converts a float to an integer
    ///
    /// This instruction requires 2 arguments:
    ///
    /// 1. The register to store the result in.
    /// 2. The register of the float to convert.
    ///
    /// # Examples
    ///
    ///     float_literals:
    ///       0: 10.5
    ///
    ///     0: set_float        0, 0
    ///     1: float_to_integer 1, 0
    fn ins_float_to_integer(&self, RcThread, RcCompiledCode, &Instruction)
        -> EmptyResult;

    /// Converts a float to a string
    ///
    /// This instruction requires 2 arguments:
    ///
    /// 1. The register to store the result in.
    /// 2. The register of the float to convert.
    ///
    /// # Examples
    ///
    ///     float_literals:
    ///       0: 10.5
    ///
    ///     0: set_float       0, 0
    ///     1: float_to_string 1, 0
    fn ins_float_to_string(&self, RcThread, RcCompiledCode, &Instruction)
        -> EmptyResult;

    /// Checks if one float is smaller than the other.
    ///
    /// This instruction requires 3 arguments:
    ///
    /// 1. The register to store the result in.
    /// 2. The register containing the float to compare.
    /// 3. The register containing the float to compare with.
    ///
    /// The result of this instruction is either boolean true or false.
    ///
    /// # Examples
    ///
    ///     float_literals:
    ///       0: 10.0
    ///       1: 15.0
    ///
    ///     0: set_float     0, 0
    ///     1: set_float     1, 1
    ///     2: float_smaller 2, 0, 1
    fn ins_float_smaller(&self, RcThread, RcCompiledCode, &Instruction)
        -> EmptyResult;

    /// Checks if one float is greater than the other.
    ///
    /// This instruction requires 3 arguments:
    ///
    /// 1. The register to store the result in.
    /// 2. The register containing the float to compare.
    /// 3. The register containing the float to compare with.
    ///
    /// The result of this instruction is either boolean true or false.
    ///
    /// # Examples
    ///
    ///     float_literals:
    ///       0: 10.0
    ///       1: 15.0
    ///
    ///     0: set_float     0, 0
    ///     1: set_float     1, 1
    ///     2: float_greater 2, 0, 1
    fn ins_float_greater(&self, RcThread, RcCompiledCode, &Instruction)
        -> EmptyResult;

    /// Checks if two floats are equal.
    ///
    /// This instruction requires 3 arguments:
    ///
    /// 1. The register to store the result in.
    /// 2. The register containing the float to compare.
    /// 3. The register containing the float to compare with.
    ///
    /// The result of this instruction is either boolean true or false.
    ///
    /// # Examples
    ///
    ///     float_literals:
    ///       0: 10.0
    ///       1: 15.0
    ///
    ///     0: set_float    0, 0
    ///     1: set_float    1, 1
    ///     2: float_equals 2, 0, 1
    fn ins_float_equals(&self, RcThread, RcCompiledCode, &Instruction)
        -> EmptyResult;

    /// Inserts a value in an array.
    ///
    /// This instruction requires 3 arguments:
    ///
    /// 1. The register containing the array to insert into.
    /// 2. The index to insert the value at.
    /// 3. The register containing the value to insert.
    ///
    /// An error is returned when the index is greater than the array length.
    ///
    /// # Examples
    ///
    ///     integer_literals:
    ///       0: 10
    ///
    ///     0: set_array    0
    ///     1: set_integer  0, 0
    ///     2: array_insert 0, 0, 0
    fn ins_array_insert(&self, RcThread, RcCompiledCode, &Instruction)
        -> EmptyResult;

    /// Gets the value of an array index.
    ///
    /// This instruction requires 3 arguments:
    ///
    /// 1. The register to store the value in.
    /// 2. The register containing the array.
    /// 3. The array index to get the value from.
    ///
    /// An error is returned when the index is greater than the array length.
    ///
    /// # Examples
    ///
    ///     integer_literals:
    ///       0: 10
    ///
    ///     0: set_array    0
    ///     1: set_integer  1, 0
    ///     2: array_insert 0, 0, 1
    ///     3: array_at     2, 0, 0
    fn ins_array_at(&self, RcThread, RcCompiledCode, &Instruction)
        -> EmptyResult;

    /// Removes a value from an array.
    ///
    /// This instruction requires 3 arguments:
    ///
    /// 1. The register to store the removed value in.
    /// 2. The register containing the array to remove a value from.
    /// 3. The index of the value to remove.
    ///
    /// An error is returned when the index is greater than the array length.
    ///
    /// # Examples
    ///
    ///     integer_literals:
    ///       0: 10
    ///
    ///     0: set_array    0
    ///     1: set_integer  1, 0
    ///     3: array_insert 0, 0, 1
    ///     4: array_remove 2, 0, 0
    fn ins_array_remove(&self, RcThread, RcCompiledCode, &Instruction)
        -> EmptyResult;

    /// Gets the amount of elements in an array.
    ///
    /// This instruction requires 2 arguments:
    ///
    /// 1. The register to store the length in.
    /// 2. The register containing the array.
    ///
    /// # Examples
    ///
    ///     0: set_array    0
    ///     1: array_length 1, 0
    fn ins_array_length(&self, RcThread, RcCompiledCode, &Instruction)
        -> EmptyResult;

    /// Removes all elements from an array.
    ///
    /// This instruction requires 1 argument: the register of the array.
    ///
    /// # Examples
    ///
    ///     integer_literals:
    ///       0: 10
    ///
    ///     0: set_integer  0, 0
    ///     1: set_array    1
    ///     2: array_insert 1, 0, 0
    ///     3: array_clear  1
    fn ins_array_clear(&self, RcThread, RcCompiledCode, &Instruction)
        -> EmptyResult;

    /// Returns the lowercase equivalent of a string.
    ///
    /// This instruction requires two arguments:
    ///
    /// 1. The register to store the new string in.
    /// 2. The register containing the input string.
    ///
    /// # Examples
    ///
    ///     string_literals:
    ///       0: "HELLO"
    ///
    ///     0: set_string      0, 0
    ///     1: string_to_lower 1, 0
    fn ins_string_to_lower(&self, RcThread, RcCompiledCode, &Instruction)
        -> EmptyResult;

    /// Returns the uppercase equivalent of a string.
    ///
    /// This instruction requires two arguments:
    ///
    /// 1. The register to store the new string in.
    /// 2. The register containing the input string.
    ///
    /// # Examples
    ///
    ///     string_literals:
    ///       0: "HELLO"
    ///
    ///     0: set_string      0, 0
    ///     1: string_to_upper 1, 0
    fn ins_string_to_upper(&self, RcThread, RcCompiledCode, &Instruction)
        -> EmptyResult;

    /// Checks if two strings are equal.
    ///
    /// This instruction requires 3 arguments:
    ///
    /// 1. The register to store the result in.
    /// 2. The register of the string to compare.
    /// 3. The register of the string to compare with.
    ///
    /// # Examples
    ///
    ///     string_literals:
    ///       0: "HELLO"
    ///       1: "hello"
    ///
    ///     0: set_string    0, 0
    ///     1: set_string    1, 1
    ///     2: string_equals 2, 0, 1
    fn ins_string_equals(&self, RcThread, RcCompiledCode, &Instruction)
        -> EmptyResult;

    /// Returns an array containing the bytes of a string.
    ///
    /// This instruction requires two arguments:
    ///
    /// 1. The register to store the result in.
    /// 2. The register containing the string to get the bytes from.
    ///
    /// # Examples
    ///
    ///     string_literals:
    ///       0: "hello"
    ///
    ///     0: set_string      0, 0
    ///     1: string_to_bytes 1, 0
    fn ins_string_to_bytes(&self, RcThread, RcCompiledCode, &Instruction)
        -> EmptyResult;

    /// Creates a string from an array of bytes
    ///
    /// This instruction requires two arguments:
    ///
    /// 1. The register to store the result in.
    /// 2. The register containing the array of bytes.
    ///
    /// The result of this instruction is either a string based on the given
    /// bytes, or an error object.
    ///
    /// # Examples
    ///
    ///     integer_literals:
    ///       0: 104
    ///       1: 101
    ///       2: 108
    ///       3: 108
    ///       4: 111
    ///
    ///     0: set_integer       0, 0
    ///     1: set_integer       1, 1
    ///     2: set_integer       2, 2
    ///     3: set_integer       3, 3
    ///     4: set_integer       4, 4
    ///     5: set_array         5, 5, 0, 1, 2, 3, 4
    ///     6: string_from_bytes 6, 5
    fn ins_string_from_bytes(&self, RcThread, RcCompiledCode, &Instruction)
        -> EmptyResult;

    /// Returns the amount of characters in a string.
    ///
    /// This instruction requires two arguments:
    ///
    /// 1. The register to store the result in.
    /// 2. The register of the string.
    ///
    /// # Examples
    ///
    ///     string_literals:
    ///       0: "hello"
    ///
    ///     0: set_string    0, 0
    ///     1: string_length 1, 0
    fn ins_string_length(&self, RcThread, RcCompiledCode, &Instruction)
        -> EmptyResult;

    /// Returns the amount of bytes in a string.
    ///
    /// This instruction requires two arguments:
    ///
    /// 1. The register to store the result in.
    /// 2. The register of the string.
    ///
    /// # Examples
    ///
    ///     string_literals:
    ///       0: "hello"
    ///
    ///     0: set_string  0, 0
    ///     1: string_size 1, 0
    fn ins_string_size(&self, RcThread, RcCompiledCode, &Instruction)
        -> EmptyResult;

    /// Writes a string to STDOUT and returns the amount of written bytes.
    ///
    /// This instruction requires two arguments:
    ///
    /// 1. The register to store the resulting object in.
    /// 2. The register containing the string to write.
    ///
    /// The result of this instruction is either an integer indicating the
    /// amount of bytes written, or an error object.
    ///
    /// # Examples
    ///
    ///     string_literals:
    ///       0: "hello"
    ///
    ///     0: set_string   0, 0
    ///     1: stdout_write 1, 0
    fn ins_stdout_write(&self, RcThread, RcCompiledCode, &Instruction)
        -> EmptyResult;

    /// Writes a string to STDERR and returns the amount of written bytes.
    ///
    /// This instruction requires two arguments:
    ///
    /// 1. The register to store the resulting object in.
    /// 2. The register containing the string to write.
    ///
    /// The result of this instruction is either an integer indicating the
    /// amount of bytes written, or an error object.
    ///
    /// # Examples
    ///
    ///     string_literals:
    ///       0: "hello"
    ///
    ///     0: set_string   0, 0
    ///     1: stderr_write 1, 0
    fn ins_stderr_write(&self, RcThread, RcCompiledCode, &Instruction)
        -> EmptyResult;

    /// Reads the given amount of bytes into a string.
    ///
    /// This instruction requires two arguments:
    ///
    /// 1. The register to store the resulting object in.
    /// 2. The register containing the amount of bytes to read.
    ///
    /// The result of this instruction is either a string containing the data
    /// read, or an error object.
    ///
    /// # Examples
    ///
    ///     integer_literals:
    ///       0: 5
    ///
    ///     0: set_integer 0, 0
    ///     1: stdin_read  1, 0
    fn ins_stdin_read(&self, RcThread, RcCompiledCode, &Instruction)
        -> EmptyResult;

    /// Reads an entire line from STDIN into a string.
    ///
    /// This instruction requires 1 argument: the register to store the
    /// resulting object in.
    ///
    /// The result of this instruction is either a string containing the read
    /// data, or an error object.
    ///
    /// # Examples
    ///
    ///     0: stdin_read_line 0
    fn ins_stdin_read_line(&self, RcThread, RcCompiledCode, &Instruction)
        -> EmptyResult;

    /// Opens a file handle in a particular mode (read-only, write-only, etc).
    ///
    /// This instruction requires X arguments:
    ///
    /// 1. The register to store the resulting object in.
    /// 2. The path to the file to open.
    /// 3. The register containing a string describing the mode to open the
    ///    file in.
    ///
    /// The result of this instruction is either a file object or an error
    /// object.
    ///
    /// The available file modes supported are the same as those supported by
    /// the `fopen()` system call, thus:
    ///
    /// * r: opens a file for reading only
    /// * r+: opens a file for reading and writing
    /// * w: opens a file for writing only, truncating it if it exists, creating
    ///   it otherwise
    /// * w+: opens a file for reading and writing, truncating it if it exists,
    ///   creating it if it doesn't exist
    /// * a: opens a file for appending, creating it if it doesn't exist
    /// * a+: opens a file for reading and appending, creating it if it doesn't
    ///   exist
    ///
    /// # Examples
    ///
    ///     string_literals:
    ///       0: "/etc/hostname"
    ///       1: "r"
    ///
    ///     0: set_string 0, 0
    ///     1: set_string 1, 1
    ///     2: file_open  2, 0, 1
    fn ins_file_open(&self, RcThread, RcCompiledCode, &Instruction)
        -> EmptyResult;

    /// Writes a string to a file.
    ///
    /// This instruction requires 3 arguments:
    ///
    /// 1. The register to store the amount of written bytes in.
    /// 2. The register containing the file object to write to.
    /// 3. The register containing the string to write.
    ///
    /// The result of this instruction is either the amount of written bytes or
    /// an error object.
    ///
    /// # Examples
    ///
    ///     string_literals:
    ///       0: "/etc/hostname"
    ///       1: "a"
    ///       2: "127.0.0.1 cats"
    ///
    ///     0: set_string 0, 0
    ///     1: set_string 1, 1
    ///     2: set_string 2, 1
    ///     2: file_open  3, 0, 1
    ///     3: file_write 4, 3, 2
    fn ins_file_write(&self, RcThread, RcCompiledCode, &Instruction)
        -> EmptyResult;

    /// Reads a number of bytes from a file.
    ///
    /// This instruction takes 3 arguments:
    ///
    /// 1. The register to store the resulting object in.
    /// 2. The register containing the file to read from.
    /// 3. The register containing the amount of bytes to read, if left out
    ///    all data is read instead.
    ///
    /// The result of this instruction is either a string containing the data
    /// read, or an error object.
    ///
    /// # Examples
    ///
    ///     string_literals:
    ///       0: "/etc/hostname"
    ///       1: "r"
    ///
    ///     integer_literals:
    ///       0: 32
    ///
    ///     0: set_string  0, 0
    ///     1: set_string  1, 1
    ///     2: set_integer 2, 0
    ///     3: file_open   3, 0, 1
    ///     4: file_read   4, 3, 2
    fn ins_file_read(&self, RcThread, RcCompiledCode, &Instruction)
        -> EmptyResult;

    /// Reads an entire line from a file.
    ///
    /// This instruction requires two arguments:
    ///
    /// 1. The register to store the resulting object in.
    /// 2. The register containing the file to read from.
    ///
    /// The result of this instruction is either a string containing the read
    /// line, or an error object.
    ///
    /// # Examples
    ///
    ///     string_literals:
    ///       0: "/etc/hostname"
    ///       1: "r"
    ///
    ///     0: set_string     0, 0
    ///     1: set_string     1, 1
    ///     2: file_open      2, 0, 1
    ///     3: file_read_line 3, 2
    fn ins_file_read_line(&self, RcThread, RcCompiledCode, &Instruction)
        -> EmptyResult;

    /// Flushes a file.
    ///
    /// This instruction requires two arguments:
    ///
    /// 1. The register to store the result in.
    /// 2. the register containing the file to flush.
    ///
    /// The resulting object is either boolean true (upon success), or an error
    /// object.
    ///
    /// # Examples
    ///
    ///     string_literals:
    ///       0: "/etc/hostname"
    ///       1: "w"
    ///       2: "127.0.0.1 localhost"
    ///
    ///     0: set_string 0, 0
    ///     1: set_string 1, 1
    ///     2: set_string 2, 2
    ///     3: file_open  3, 0, 1
    ///     4: file_write 4, 3, 2
    ///     5: file_flush 5, 3
    fn ins_file_flush(&self, RcThread, RcCompiledCode, &Instruction)
        -> EmptyResult;

    /// Returns the size of a file in bytes.
    ///
    /// This instruction requires two arguments:
    ///
    /// 1. The register to store the resulting object in.
    /// 2. The register containing the file.
    ///
    /// The resulting object is either an integer representing the amount of
    /// bytes, or an error object.
    ///
    /// # Examples
    ///
    ///     string_literals:
    ///       0: "/etc/hostname"
    ///       1: "r"
    ///
    ///     0: set_string 0, 0
    ///     1: set_string 1, 1
    ///     2: file_open  2, 0, 1
    ///     3: file_size  3, 2
    fn ins_file_size(&self, RcThread, RcCompiledCode, &Instruction)
        -> EmptyResult;

    /// Sets a file cursor to the given offset in bytes.
    ///
    /// This instruction requires 3 arguments:
    ///
    /// 1. The register to store the resulting object in.
    /// 2. The register containing the input file.
    /// 3. The offset to seek to as an integer.
    ///
    /// The resulting object is either an integer representing the new cursor
    /// position, or an error object.
    ///
    /// # Examples
    ///
    ///     string_literals:
    ///       0: "/etc/hostname"
    ///       1: "r"
    ///
    ///     integer_literals:
    ///       0: 10
    ///
    ///     0: set_string  0, 0
    ///     1: set_string  1, 1
    ///     2: set_integer 2, 0
    ///     3: file_open   3, 0, 1
    ///     4: file_seek   4, 3, 2
    fn ins_file_seek(&self, RcThread, RcCompiledCode, &Instruction)
        -> EmptyResult;

    /// Parses and runs a given bytecode file using a string literal
    ///
    /// Files are executed only once. After a file has been executed any
    /// following calls are basically no-ops.
    ///
    /// This instruction requires 2 arguments:
    ///
    /// 1. The register to store the resulting object in.
    /// 2. The string literal index containing the file path of the bytecode
    ///    file.
    ///
    /// The result of this instruction is whatever the bytecode file returned.
    ///
    /// # Examples
    ///
    ///     string_literals:
    ///       0: "/tmp/test.abc"
    ///
    ///     0: run_file_fast 0, 0
    fn ins_run_file(&self, RcThread, RcCompiledCode, &Instruction)
        -> EmptyResult;

    /// Parses and runs a given bytecode file using a runtime allocated string
    ///
    /// This instruction takes the same arguments as the "run_file" instruction
    /// except instead of using a string literal it uses a register
    /// containing a runtime allocated string.
    fn ins_run_file_dynamic(&self, RcThread, RcCompiledCode, &Instruction)
        -> EmptyResult;

    /// Prints a VM backtrace of a given thread with a message.
    fn error(&self, RcThread, String);

    /// Runs a given CompiledCode with arguments.
    fn run_code(&self, RcThread, RcCompiledCode, RcObject, Vec<RcObject>,
                Option<RcBinding>) -> OptionObjectResult;

    /// Runs a bytecode file.
    fn run_file(&self, &String, RcThread, usize) -> EmptyResult;

    /// Sends a message to an object
    fn send_message(&self, &String, RcThread, &Instruction) -> EmptyResult;

    /// Collects a set of arguments from an instruction.
    fn collect_arguments(&self, RcThread, &Instruction, usize, usize)
        -> ObjectVecResult;

    /// Runs a CompiledCode in a new thread.
    fn start_thread(&self, RcCompiledCode) -> RcObject;

    /// Runs a CompiledCode using an existing thread.
    fn run_thread(&self, RcObject, RcCompiledCode);
}
