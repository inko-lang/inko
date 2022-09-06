# Foreign Function Interface

Inko supports interacting with C through it's Foregin Function Interface (FFI),
powered by [libffi](https://sourceware.org/libffi/).

!!! note
    Inko's FFI is quite basic and comes with various limitations. We aim to
    improve this over time.

Inko's FFI is provided using the module `std::ffi`. For more information, refer
to the source code of this module.

## Loading libraries

To load a library at runtime, use the type `std::ffi::Library`:

```inko
import std::ffi::Library

class async Main {
  fn async main {
    let lib = Library
      .new(['libc.so', 'libc.so.6', 'libSystem.dylib', 'msvcrt.dll'])
      .unwrap
  }
}
```

`Library.new` tries to open any of the given library names or paths, returning a
`Some(Library)` if the library is found.

When the `Library` value is dropped, the handle to the C library is closed
automatically.

## Loading functions

After creating a `Library` you can load functions through it. For example,
`malloc` and `free` are loaded as follows:

```inko
import std::ffi::(Library, Type)

class async Main {
  fn async main {
    let lib = Library
      .new(['libc.so', 'libc.so.6', 'libSystem.dylib', 'msvcrt.dll'])
      .unwrap

    let malloc = lib
      .function('malloc', arguments: [Type.SizeT], returns: Type.Pointer)
      .unwrap

    let free = lib
      .function('free', arguments: [Type.Pointer], returns: Type.Void)
      .unwrap
  }
}
```

Similar to `Library.new`, `Library.function` returns an `Option`, with a `None`
signalling the function doesn't exist.

## Loading variables

Similar to loading functions we can load variables:

```inko
import std::ffi::(Library, Type)

class async Main {
  fn async main {
    let lib = Library
      .new(['libc.so', 'libc.so.6', 'libSystem.dylib', 'msvcrt.dll'])
      .unwrap

    let errno = lib.variable('errno').unwrap
  }
}
```

Note that C libraries tend to implement (global) variables using macros, in
which case `Library.variable` won't be able to find the variable. For example,
[musl](https://www.musl-libc.org/) defines `errno` as a macro, which calls a
function under the hoods.

## Calling functions

Calling functions is done using methods such as `Function.call0`,
`Function.call1`, etc. The arguments of these methods are of type `Any`, and
only the following values can be passed to these arguments:

- `String`
- `Int`
- `Float`
- `ByteArray`
- Any `Any` produced by C code (e.g. the return value of `malloc` in the above
  example)

Inko performs no type-checking whatsoever on values of type `Any`, so you have
to be careful when using such values. Incorrect use of `Any` values may lead to
the program crashing, eating your laundry, or work just fine until Friday
afternoon just as you're about to leave for home, at which point everything
catches fire.

When using the return value of a C function, it's up to you to cast it to or
construct the appropriate type. Consider our `malloc` example from earlier: it's
return type is a pointer (specified using `returns: Type.Pointer`), for which
`std::ffi` provides the type `Pointer`. This means we can use the result like
so:

```inko
import std::ffi::(Library, Type, Pointer)

class async Main {
  fn async main {
    let lib = Library
      .new(['libc.so', 'libc.so.6', 'libSystem.dylib', 'msvcrt.dll'])
      .unwrap

    let malloc = lib
      .function('malloc', arguments: [Type.SizeT], returns: Type.Pointer)
      .unwrap

    let free = lib
      .function('free', arguments: [Type.Pointer], returns: Type.Void)
      .unwrap

    let mem = Pointer.new(malloc.call1(32))
  }
}
```

The `Pointer` type is a regular Inko type, so we can use it and pass it around
like any other value. If we want to pass it back to C, we have to get the
underlying raw pointer using `Pointer.raw`:

```inko
import std::ffi::(Library, Type, Pointer)

class async Main {
  fn async main {
    let lib = Library
      .new(['libc.so', 'libc.so.6', 'libSystem.dylib', 'msvcrt.dll'])
      .unwrap

    let malloc = lib
      .function('malloc', arguments: [Type.SizeT], returns: Type.Pointer)
      .unwrap

    let free = lib
      .function('free', arguments: [Type.Pointer], returns: Type.Void)
      .unwrap

    let mem = Pointer.new(malloc.call1(32))

    free.call1(mem.raw)
  }
}
```

If the return type is a built-in type such as `String` or `Int`, we can cast the
`Any` to the appropriate type:

```inko
import std::ffi::(Library, Type, Pointer)

class async Main {
  fn async main {
    let lib = Library
      .new(['libc.so', 'libc.so.6', 'libSystem.dylib', 'msvcrt.dll'])
      .unwrap

    let atol = lib
      .function('atol', arguments: [Type.String], returns: Type.I64)
      .unwrap

    atol.call1('123') as Int # => 123
  }
}
```

If you cast an `Any` to the wrong type, all hell breaks loose, so be careful!

## Defining structures

The FFI module supports the means to build wrappers around structures, making it
easier to read and write their fields. This is done using the `LayoutBuilder`
and `Struct` types. A `LayoutBuilder` is used to construct the layout of a
struct, represented using the `Layout` type. A `Struct` wraps a pointer to a C
structure, and a corresponding `Layout`.

For example, if you have a struct with two `int` fields (`Type.I32`), you'd
define the layout like so:

```inko
import std::ffi::(LayoutBuilder, Struct)

let builder = LayoutBuilder.new

builder.field('foo', Type.I32)
builder.field('bar', Type.I32)

let layout = builder.into_layout
let struct = Struct.new(pointer_to_the_c_struct, layout)

struct['foo']
struct['foo'] = 42
```

The struct padding/alignment is calculated automatically, and can be disabled
using `LayoutBuilder.no_padding`. Memory is still managed manually, so don't
forgot to somehow free the structure when you're done with it.

## Memory management

Inko's FFI doesn't automatically manage memory of C values, besides closing a
loaded library when dropping a `Library` value. This means its your own
responsibility to release memory (e.g. by using C's `free()` function) whenever
necessary.

## Limitations

- C calling back into Inko isn't supported.
- Variadic functions aren't supported.
- The `Function` type only supports calling of functions with up to six
  arguments, at least for now.
- Inko doesn't support pinning of processes to OS threads (this is by design),
  making it more difficult to interact with C code that uses thread-local
  storage. See [this issue](https://gitlab.com/inko-lang/inko/-/issues/258) for
  more information.
- C code that requires to be run on the main thread should be called from the
  "Main" process, as this process is always run on the main thread. There's no
  support for forcing other processes to run on the main thread.
