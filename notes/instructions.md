# Virtual Machine Instructions

The Aeon virtual machine is a register based bytecode VM. Each call frame in a
program has its own register that instructions can set/get temporary values
from. Literals such as integers and strings are stored in "compiled code"
objects and can be stored in a register (as a proper Aeon object) using certain
instructions.

## Available Instructions

The following instructions are supported by the VM:

* set_object
* set_integer
* set_float
* set_string
* set_array
* set_lvar
* get_lvar
* set_const
* get_const
* set_ivar
* get_ivar
* send
* return
* get_cscope
* get_tscope
* goto_if_undef

There is no dedicated instruction for setting a Hash/Dictionary as this can (or
at least should) be implemented in Aeon itself.

There are also no instructions for flow control in the VM. Because Aeon uses
methods for flow control (just like Smalltalk) there is no need for a set of
dedicated instructions.

The bytecode will be serialized to files using either msgpack or cap'n proto, I
haven't decided yet. Either way it won't be a home grown format.

## set_object

Allocates a new regular object and stores it in the given slot.

Signature:

    set_object TARGET_SLOT

Example:

    set_object 0

## set_integer

Takes an integer from the integer literals list, allocates it as a proper Aeon
integer and stores it in a register slot.

Signature:

    set_integer TARGET_SLOT, INTEGER_LITERAL_SLOT

Example:

    set_integer 0, 0

## set_float

Takes a float from the float literals list, allocates it as a proper Aeon float
and stores it in a register slot.

Signature:

    set_float TARGET_SLOT, FLOAT_LITERAL_SLOT

Example:

    set_float 0, 0

## set_string

Takes a string from the string literals list, allocates it as a proper Aeon
string and stores it in a register slot.

Signature:

    set_string TARGET_SLOT, STRING_LITERAL_SLOT

Example:

    set_string 0, 0

## set_array

Allocates a new array populated with the values in the given register slots.

Signature:

    set_array TARGET_SLOT, VALUE_COUNT, VALUE_SLOT1, VALUE_SLOT2, ...

Example:

    set_integer 0, 0
    set_float   1, 0
    set_array   2, 2, 0, 1

This creates an Array filled with the integer in slot 0 and the float in slot 1.

## set_lvar

Sets a local variable to the value in the given slot.

Signature:

    set_lvar LOCAL_SLOT, VALUE_SLOT

Example:

    Integers:
      0: 10

    set_integer 0, 0
    set_lvar    0, 0 # number

This would be produced by code such as the following:

    number = 10

The names of local variables are not present in the bytecode. It's up to the
compiler to map variable names to the correct local variable slots.

## get_lvar

Gets the value of a local variable and puts it in a register slot.

Signature:

    get_lvar TARGET_SLOT, LOCAL_SLOT

Example:

    Locals:
      0: "example"

    get_local 0, 0

## set_const

Sets a constant to the value of the given register slot. Because constants are
shared in child scopes they are looked up using a name instead of an index.

As contants can be set as child constants of another (e.g. `A::B = 10`) this
instruction requires an explicit scope object to set the constant in.

Constants can be set as following:

    A    = 10 # current scope
    ::A  = 10 # top-level scope
    A::B = 10 # in the A constant

For `A = 10` the instructions are as following:

    Literals:
      0: "A"

    Integers:
      0: 10

    get_cscope 0
    set_const  0, 0, 0

For `::A = 10` the instructions are as following:

    Literals:
      0: "A"

    Integers:
      0: 10

    get_tscope 0
    set_const  0, 0, 0

For `A::B = 10` the instructions are as following:

    Literals:
      0: "A"
      1: "B"

    Integers:
      0: 10

    get_cscope 0
    get_const  1, 0
    set_const  1, 1, 0

For `::A::B = 10` the instructions are as following:

    Literals:
      0: "A"
      1: "B"

    Integers:
      0: 10

    get_tscope 0
    get_const  1, 0
    set_const  1, 1, 0

Thus the signature is as following:

    set_const SCOPE_SLOT, NAME_LITERAL_SLOT, VALUE_SLOT

Here `SCOPE_SLOT` refers to the slot that contains the object in which to set
the constant.

NOTE: the get_tscope and get_cscope instructions might be removed in the end.

## get_const

TODO

## set_ivar

TODO

## get_ivar

TODO

## send

Sends a message to a receiver and stores the results in a register slot.

Signature:

    send TARGET_SLOT, RECEIVER_SLOT, NAME_LITERAL_SLOT, ALLOW_PRIVATE,
         ARG_AMOUNT, ARG_SLOT1, ...

Example:

    Integers:
      0: 10
      1: 2

    Literals:
      0: "*"

    set_integer 0, 0
    set_integer 1, 1
    send        2, 0, 0, 0, 1, 1

                ^  ^  ^
                |  |  |
                |  |  |
           +----+  |  +------+
           |       |         |
         result  receiver  name

This would be produced by the following code:

    10 * 2

Which is the same as this:

    10.*(2)

Because allocating strings when sending a message using a literal name is a
waste a compiled code object would have to store the literal names. These names
are in turn used when sending messages.

The Aeon VM supports 3 types of arguments when sending messages:

* Positional arguments
* Named arguments
* Rest arguments

Positional arguments are set in order:

    example(1, 2, 3)

Named arguments are set using the name of the argument, the order does not
matter:

    example(a = 1, c = 3, b = 2)

Named arguments are simply mapped to their positional equivalents.

A rest argument is an argument defined as `*argment` and consumes all arguments
that were not assigned to positional arguments. For example, consider the
following method definition:

    def example(a, *other) { }

When called using `example(a)` the `other` variable should be set to an empty
array. When called using `example(a, b, c, d)` the `other` variable should be
set to the array `[b, c, d]`.

When a `send` instruction is invoked it will lookup the compiled code object of
the method or raise a VM error if the method doesn't exist. It's up to
compilers/languages to delegate non existing methods to a catch-all handler if
needed. Once the compiled code object is found a new call frame is created and
the arguments are set as local variables in the new call frame's register. These
variables should be set in the same order as they are defined as arguments.

## return

TODO

## get_cscope

TODO

## get_tscope

## goto_if_undef

Jumps to the given instruction if a register slot is not set.

Signature:

    goto_if_undef INSTRUCTION_INDEX, VALUE_SLOT

Example:

    Locals:
      0: foo

    Integers:
      0: 10

    0: get_local     0, 0 # foo
    1: goto_if_undef 4, 0
    2: set_integer   1, 0 # 10
    3: set_local     0, 1 # foo = 10
    4: return        0

This is equivalent to the following pseudo code:

    if !foo {
        foo = 10
    }

    return foo

A more expanded example:

    def add(left, right = 10) -> Numeric {
        left + right
    }

This would produce:

    Arguments:
      required: 1

    Locals:
      0: left
      1: right

    Literals:
      0: "+"

    Integers:
      0: 10

    Instructions:
      0: get_local     0, 1          # right
      1: goto_if_undef 4, 0
      2: set_integer   1, 0          # 10
      3: set_local     1, 1          # right = 10
      4: get_local     2, 0          # left
      5: send          4, 2, 0, 1, 0 # left + right
      6: return        4
