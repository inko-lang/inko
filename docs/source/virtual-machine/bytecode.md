# Bytecode

The VM executes bytecode, instead of traversing an AST. Instead of executing a
bytecode file for every module, all modules are compiled into a single bytecode
file, known as a "bytecode image".

The format in which bytecode is serialised is a custom binary format. A bytecode
image is broken up into two sections: a header, and a list of modules that make
up the program to run. Each module in turn consists out of one or more compiled
code objects.

A compiled code object is a collection of instructions and meta data describing
a single Inko block, such as a method. These objects include the name, the path
of the source file, the instructions to run, debugging information, and more.
Each compiled code object can contain 0 or more other compiled code objects that
may need to be run.

At various points in this guide will we reference certain types such as `u8` or
`i64`. These types are defined as follows:

| Type      | Meaning
|:----------|:---------------------------------------------------------------
| `u8`      | An 8 bits unsigned integer.
| `u16`     | A 16 bits unsigned integer, serialised in big-endian order.
| `u64`     | A 64 bits unsigned integer, serialised in big-endian order.
| `i64`     | A 64 bits signed integer, serialised in big-endian order.
| `[X; Y]`  | A fixed size array, containing `Y` values of type `X`, such as `[u8; 4]`.
| `boolean` | A single `u8` that can only be `0` or `1`.

In certain places we also use examples such as `[1, 2, 3]`. This means we are
referring to an array containing the values `1, 2, 3` in the given order.

## Header

Every bytecode image must start with a header. The header consists out of two
parts:

1. A signature.
1. The version of the bytecode format.

The signature is a `[u8; 4]` containing the following `u8` values:

    [105, 110, 107, 111]

When converted to a string, this will read "inko".

The version is used by the VM to determine if it will be able to parse the
bytecode file. The version is a single `u8`, and is only incremented when
backwards incompatible bytecode changes are made. The version byte comes
directly after the signature.

If the signature or version is not recognised, the VM will exit with an error.

## Modules

After the header comes the list of modules. First there is a `u64` that contains
the number of modules included in the image. Each module consists of two
sections:

1. A list of all literals used by the module.
1. The compiled code object for the module's body.

The list of literals starts with a `u64` containing the total number of
literals. The maximum number of literals is `(1 << 32) - 1`. This `u64` is then
followed by the literals.

## Literals

Literals are values such as string, integer and float literals. These literals
are stored at the module-level. Thus, if the same literal is referred to 100
times, it's only stored once; reducing the size of both the bytecode image and
the memory used. When referring to these literals, the bytecode uses an index to
the module-level literals table containing that literals.

The following types of literals are stored in the literals table:

* Integers
* Byte arrays
* Big integers
* Floats
* Strings

Each literal starts with a `u8` that specifies the type of literal. This byte is
then followed by one or more bytes that make up the literal.

### Integers

Integers are serialised as a `u8` of value `0`, followed by a `[u8; 8]`
containing the bytes that make up the integer. For example, the integer `42` is
serialised as:

```inko
[0, 0, 0, 0, 0, 0, 0, 0, 42]
```

The maximum value that can be serialised as an integer is
`9 223 372 036 854 775 807`.

The values are ordered in big-endian order.

### Big integers

Big integers start with a `u8` of value `3`, followed by a hexadecimal string
literal. For example, the number `18 446 744 073 709 551 614` is serialised as
follows:

```inko
[
    3,                                      # The type marker of a big integer
    16, 0, 0, 0, 0, 0, 0, 0,                # The number of bytes in the string
    102, 102, 102, 102, 102, 102, 102, 102,
    102, 102, 102, 102, 102, 102, 102, 101
]
```

Serialising a big integer is done as follows:

1. Take the integer value
1. Convert it to a hexadecimal string
1. Serialize this as a bytecode string literal
1. Prefix it with the byte value `3`

### Floats

Floats start with a `u8` of value `1`, followed by a `[u8; 8]`. For example, the
float 15.2 is serialised as follows:

```inko
[
  1,                                    # The type marker of a float
  64, 46, 102, 102, 102, 102, 102, 102  # The bytes that make up the float
]
```

The virtual machine parses this into a float by reading the bytes, then uses
these directly as the bits layout for the float. In Rust this is done using
`std::f64::from_bits()`.

The bytes of a float are ordered in big-endian order.

### Strings

Strings start with a `u8` of value `2`, followed by a `u64` indicating the
number of _bytes_ in the string, followed by a sequence of `u8` values that make
up the string.

The string "inko" is serialised as follows:

```inko
[
  2,                      # The type indicator for a string
  0, 0, 0, 0, 0, 0, 0, 4, # The number of bytes
  105, 110, 107, 111      # The bytes in the string
]
```

## Compiled code

Compiled code objects are a bit complex to parse as they contain quite a bit of
data. Each compiled code object has the following fields (all are required),
parsed in this order:

1. The name of the object, as a string.
1. The path of the source file, as a string.
1. The line number the code object originates from, as a `u16`.
1. The names of the arguments as an array of strings, empty if no arguments are
   defined.
1. A `u8` indicating the number of required arguments.
1. The number of local variables used by the compiled code object, as a `u16`.
1. The number of registers used by the compiled code object, as a `u16`.
1. A `boolean` indicating if the compiled code object captures any outer local
   variables.
1. An array of 0 or more instructions.
1. An array of compiled code objects defined inside this compiled code object.
1. An array containing 0 or more catch entries.

## Instructions

Each VM instruction consists out of the following fields, in this order:

1. A `u8` indicating the instruction to execute.
1. A `u16` specifying the line the instruction originates from.
1. A `[u16; 6]` containing the instruction arguments. If an argument is unset,
   its value is `0`.

Each instruction has a size of 16 bytes.

### Available instructions

The following instruction types and their `u8` values are available:

| Instruction             | Byte
|:------------------------|:-----------
| SetLiteral              | 0
| SetLiteralWide          | 1
| Allocate                | 2
| AllocatePermanent       | 3
| ArrayAllocate           | 4
| GetBuiltinPrototype     | 5
| GetTrue                 | 6
| GetFalse                | 7
| SetLocal                | 8
| GetLocal                | 9
| SetBlock                | 10
| Return                  | 11
| GotoIfFalse             | 12
| GotoIfTrue              | 13
| Goto                    | 14
| RunBlock                | 15
| IntegerAdd              | 16
| IntegerDiv              | 17
| IntegerMul              | 18
| IntegerSub              | 19
| IntegerMod              | 20
| IntegerToFloat          | 21
| IntegerToString         | 22
| IntegerBitwiseAnd       | 23
| IntegerBitwiseOr        | 24
| IntegerBitwiseXor       | 25
| IntegerShiftLeft        | 26
| IntegerShiftRight       | 27
| IntegerSmaller          | 28
| IntegerGreater          | 29
| IntegerEquals           | 30
| FloatAdd                | 31
| FloatMul                | 32
| FloatDiv                | 33
| FloatSub                | 34
| FloatMod                | 35
| FloatToInteger          | 36
| FloatToString           | 37
| FloatSmaller            | 38
| FloatGreater            | 39
| FloatEquals             | 40
| ArraySet                | 41
| ArrayAt                 | 42
| ArrayRemove             | 43
| ArrayLength             | 44
| ArrayClear              | 45
| StringToLower           | 46
| StringToUpper           | 47
| StringEquals            | 48
| StringToByteArray       | 49
| StringLength            | 50
| StringSize              | 51
| StdoutWrite             | 52
| StderrWrite             | 53
| StdinRead               | 54
| FileOpen                | 55
| FileWrite               | 56
| FileRead                | 57
| FileFlush               | 58
| FileSize                | 59
| FileSeek                | 60
| ModuleLoad              | 61
| SetAttribute            | 62
| GetAttribute            | 63
| GetPrototype            | 64
| LocalExists             | 65
| ProcessSpawn            | 66
| ProcessSendMessage      | 67
| ProcessReceiveMessage   | 68
| ProcessCurrent          | 69
| SetParentLocal          | 70
| GetParentLocal          | 71
| ObjectEquals            | 72
| GetNil                  | 73
| AttributeExists         | 74
| GetAttributeNames       | 75
| TimeMonotonic           | 76
| GetGlobal               | 77
| SetGlobal               | 78
| Throw                   | 79
| CopyRegister            | 80
| TailCall                | 81
| ProcessSuspendCurrent   | 82
| IntegerGreaterOrEqual   | 83
| IntegerSmallerOrEqual   | 84
| FloatGreaterOrEqual     | 85
| FloatSmallerOrEqual     | 86
| CopyBlocks              | 87
| FloatIsNan              | 88
| FloatIsInfinite         | 89
| FloatFloor              | 90
| FloatCeil               | 91
| FloatRound              | 92
| Close                   | 93
| ProcessSetBlocking      | 94
| StdoutFlush             | 95
| StderrFlush             | 96
| FileRemove              | 97
| Panic                   | 98
| Exit                    | 99
| Platform                | 100
| FileCopy                | 101
| FileType                | 102
| FileTime                | 103
| TimeSystem              | 104
| DirectoryCreate         | 105
| DirectoryRemove         | 106
| DirectoryList           | 107
| StringConcat            | 108
| HasherNew               | 109
| HasherWrite             | 110
| HasherToHash            | 111
| Stacktrace              | 112
| ProcessTerminateCurrent | 113
| StringSlice             | 114
| BlockMetadata           | 115
| StringFormatDebug       | 116
| StringConcatMultiple    | 117
| ByteArrayFromArray      | 118
| ByteArraySet            | 119
| ByteArrayAt             | 120
| ByteArrayRemove         | 121
| ByteArrayLength         | 122
| ByteArrayClear          | 123
| ByteArrayEquals         | 124
| ByteArrayToString       | 125
| EnvGet                  | 126
| EnvSet                  | 127
| EnvVariables            | 128
| EnvHomeDirectory        | 129
| EnvTempDirectory        | 130
| EnvGetWorkingDirectory  | 131
| EnvSetWorkingDirectory  | 132
| EnvArguments            | 133
| EnvRemove               | 134
| BlockGetReceiver        | 135
| RunBlockWithReceiver    | 136
| ProcessSetPanicHandler  | 137
| ProcessAddDeferToCaller | 138
| SetDefaultPanicHandler  | 139
| ProcessSetPinned        | 140
| FFILibraryOpen          | 141
| FFIFunctionAttach       | 142
| FFIFunctionCall         | 143
| FFIPointerAttach        | 144
| FFIPointerRead          | 145
| FFIPointerWrite         | 146
| FFIPointerFromAddress   | 147
| FFIPointerAddress       | 148
| FFITypeSize             | 149
| FFITypeAlignment        | 150
| StringToInteger         | 151
| StringToFloat           | 152
| FloatToBits             | 153
| ProcessIdentifier       | 154
| SocketCreate            | 155
| SocketWrite             | 156
| SocketRead              | 157
| SocketAccept            | 158
| SocketReceiveFrom       | 159
| SocketSendTo            | 160
| SocketAddress           | 161
| SocketGetOption         | 162
| SocketSetOption         | 163
| SocketBind              | 164
| SocketListen            | 165
| SocketConnect           | 166
| SocketShutdown          | 167
| RandomNumber            | 168
| RandomRange             | 169
| RandomBytes             | 170
| StringByte              | 171
| ModuleList              | 172
| ModuleGet               | 173
| ModuleInfo              | 174
| GetAttributeInSelf      | 175
| MoveResult              | 176
| FilePath                | 177

### Variable-length arguments

Since instructions have a fixed size, they can only support a fixed number of
arguments (six to be exact). Some instructions need to operate on more than six
values, such as the `SetArray` instruction. Such instructions do this as
follows:

1. One argument specifies the register containing the first value.
1. One argument is used to specify the number of values.
1. All other values follow the first one.

As an example, consider the following Inko array:

```inko
Array.new('a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'i', 'j')
```

This array has 10 values, which don't fit in a single instruction. The resulting
`ArrayAllocate` bytecode would look something like this:

    %a = 'a'
    %b = 'b'
    %c = 'c'
    %d = 'd'
    %e = 'e'
    %f = 'f'
    %g = 'g'
    %h = 'h'
    %i = 'i'
    %j = 'j'
    %result = ArrayAllocate(%a, 10)

Here `%result` is the register to store the resulting array in. `%a` is the
first register, followed by the registers containing the other values.

## Catch entries

A catch entry specifies a sequence of instructions that may throw an error, and
what instruction to jump to when this happens. Each entry consists out of the
following fields:

1. A `u16` containing the start position of the instruction range.
1. A `u16` containing the end position of the instruction range.
1. A `u16` containing the instruction position to jump to.
1. A `u16` containing the register to store the error value in.

Instructions are zero-indexed, meaning the first instruction starts at index
`0`.
