---
{
  "title": "Interacting with C"
}
---

Sometimes you are in need of certain functionality for which no Inko library
exists, but a library written in C providing the functionality does. Inko's
foreign function interface allows you to use C libraries with almost no runtime
overhead.

Before we dive into how to use the FFI, there are a few things to keep in mind.

First, when using the FFI, Inko's safety guarantees are thrown out of the
window, as the compiler can't verify if C function calls are safe (e.g. they
don't mutate global state).

Second, Inko won't manage memory allocated through C functions, meaning you need
to manually release it yourself (e.g. using `free()`). This means you may run
into memory leaks if you're not careful.

Third, the API comes with several limitations (further discussed below), which
may complicate using certain C libraries.

In short: you should avoid using C code as much as you can. When you _do_ need
to use it, be careful as it's easy to make mistakes.

::: warn
We'll say it again just to be clear: avoid using C code unless you have
determined there's no other option.
:::

With that out of the way, let's get started.

## Foreign types

When interacting with C code, we need to work with types that are specific to C,
such as pointers. Inko's FFI offers the following types, along with their C
equivalents:

|=
| Inko type
| Size (bits)
| C type
|-
| `Int8`
| 8
| `int8_t`
|-
| `Int16`
| 16
| `int16_t`
|-
| `Int32`
| 32
| `int32_t`
|-
| `Int64`
| 64
| `int64_t`
|-
| `UInt8`
| 8
| `uint8_t`
|-
| `UInt16`
| 16
| `uint16_t`
|-
| `UInt32`
| 32
| `uint32_t`
|-
| `UInt64`
| 64
| `uint64_t`
|-
| `Float32`
| 32
| `float`
|-
| `Float64`
| 64
| `double`
|-
| `Pointer[T]`
| 64[^1]
| `T*`

Pointer-pointers don't have a dedicated type (i.e. there's no
`Pointer[Pointer[Int8]]`), instead they are represented as just regular pointers
(e.g. `Pointer[Int8]`).

There's no equivalent of C's `size_t` type, as Inko only supports 64-bits
platforms, and thus you can just use `UInt64` instead.

C types don't support methods or operators, nor can they be passed to
generically typed values/arguments. For example, `Array[Int32]` isn't a valid
type. In practise this means you'll need to cast or wrap C types before you can
do anything with them, apart from passing them around.

::: note
C types not being compatible with generics is a limitation due to how
generics are compiled. This is unlikely to change any time soon, if ever.
:::

C types are treated as value types and are allocated on the stack, including
structs (which we'll discuss later).

### Type casting

These types can be passed to/from Inko types:

```inko
let a = 42 as Int32 # => Int32
let b = a as Int    # => Int
```

Inko doesn't perform implicit type casts, so passing an `Int32` when a `Int64`
is expected will result in a compile-time error. Similarly, pointers of type
`Pointer[A]` aren't implicitly compatible with pointers of type `Pointer[B]`,
and instead require an explicit type cast.

## Importing libraries

Libraries to link against are specified using the `import extern "NAME"` syntax.
For example, to import libm:

```inko
import extern "m"
```

The name should be the library name without any prefix (e.g. "lib"), file
extension or file path. To illustrate, let's take this simple program and save
it as `test.inko`:

```inko
import extern "m"

class async Main {
  fn async main {}
}
```

Then run the following:

```bash
inko build -o /tmp/test test.inko
ldd /tmp/test
```

On a GNU Linux system this outputs:

```
linux-vdso.so.1 (0x00007ffdcc7a6000)
libm.so.6 => /lib64/libm.so.6 (0x00007feb7eb59000)
libc.so.6 => /lib64/libc.so.6 (0x00007feb7e97b000)
/lib64/ld-linux-x86-64.so.2 (0x00007feb7ec48000)
```

### Dynamic vs static linking

By default, C libraries are linked _dynamically_. This is done because while
dynamic libraries are widely available, not all platforms provide static
equivalents, or require extra steps. For example, on many Linux distributions
installing the package "foo" only gives you a dynamic library, requiring an
extra "foo-static" package to be installed to also get the static library.

If you happen to have all the necessary static libraries installed, you can
instruct the compiler to link them statically using `inko build --static`. This
flag applies to all libraries, meaning we either link _all_ of them dynamically
_or_ statically. Inko doesn't support dynamically linking some libraries while
statically linking others.

::: note
libc and libm are always dynamically linked (even with the `--static` flag),
_unless_ you are using a platform that defaults to static linking them, such
as Alpine Linux.

Some platforms merge libc and libm together, such as macOS. In this case
Inko only links against libc.
:::

To illustrate static linking, we'll update our `test.inko` to import zlib
instead:

```inko
import extern "z"

class async Main {
  fn async main {}
}
```

Then we build it and show what libraries the executable is linked against:

```bash
inko build -o /tmp/test test.inko
ldd /tmp/test
        linux-vdso.so.1 (0x00007ffd8eb8f000)
        libm.so.6 => /lib64/libm.so.6 (0x00007fb87d9c4000)
        libz.so.1 => /lib64/libz.so.1 (0x00007fb87d9aa000)
        libc.so.6 => /lib64/libc.so.6 (0x00007fb87d7cc000)
        /lib64/ld-linux-x86-64.so.2 (0x00007fb87dab3000)
```

On Fedora Linux, we can get a static version of zlib by running
`sudo dnf install zlib-static`, after which we can use the `--static` flag to
link statically against zlib:

```bash
inko build --static -o /tmp/test test.inko
ldd /tmp/test
        linux-vdso.so.1 (0x00007ffc06993000)
        libm.so.6 => /lib64/libm.so.6 (0x00007fbb76e50000)
        libc.so.6 => /lib64/libc.so.6 (0x00007fbb76c72000)
        /lib64/ld-linux-x86-64.so.2 (0x00007fbb76f3f000)
```

## Functions

Importing libraries alone isn't useful, so let's define bindings for some
functions from the libm library and use them. Defining the signatures of C
functions is done using `fn extern`. For example, if we want to use the `ceil()`
function from libm, we'd define the signature as follows:

```inko
fn extern ceil(value: Float64) -> Float64
```

If a signature is defined without a return type, the return type is inferred as
the C type `void` (i.e. no value is returned).

C functions are called like regular Inko methods:

```inko
import std.stdio.STDOUT
import extern "m"

fn extern ceil(value: Float64) -> Float64

class async Main {
  fn async main {
    let out = STDOUT.new

    # Float64 is a C type and we can't call methods on such types, so we must
    # explicitly cast the returned value to Float (Inko's floating point type).
    let val = ceil(1.123 as Float64) as Float

    out.print(val.to_string)
  }
}
```

When running this program, the output will be `2.0`.

::: tip
If a C function defines an argument of type `Int`, Inko treats this as
`Int64` and implicitly converts `Int` arguments to `Int64` arguments. This
is only true for `Int` arguments, and return types should be `Int64` and
`Float64` instead of `Int` and `Float` respectively, as `Int` and `Float`
have a different memory representation at the moment.
:::

::: warn
Don't use types such as `ref T` and `mut T` in signatures. While this is
supported, it's used to interact with Inko's runtime library written in
Rust, and shouldn't be used outside of the standard library.
:::

Variadic functions are also supported, and are defined as follows:

```inko
fn extern printf(format: Pointer[UInt8], ...) -> Int32

class async Main {
  fn async main {
    printf("Hello %s\n".to_pointer, "Inko".to_pointer)
  }
}
```

This program prints "Hello Inko" to STDOUT.

::: warn
When using variadic functions, the compiler doesn't type-check any of the
additional arguments, because it doesn't know what the expected types are.
:::

## Structures

Inko supports defining signatures for C structures, similar to classes. This is
done using the `class extern` syntax. For example, to define the `timespec`
structure from the libc `time.h` header, we'd write the following:

```inko
class extern Timespec {
  let @tv_sec: Int64
  let @tv_nsec: Int64
}
```

Like classes, we can create instances of these structs:

```inko
class extern Timespec {
  let @tv_sec: Int64
  let @tv_nsec: Int64
}

class async Main {
  fn async main {
    Timespec { @tv_sec = 123 as Int64, @tv_nsec = 456 as Int64 }
  }
}
```

Structures are allocated on the stack and are value types, meaning a move
results in a copy (unless this is optimised away). Reading and writing of
structure fields uses the same syntax as regular Inko classes:

```inko
class extern Timespec {
  let @tv_sec: Int64
  let @tv_nsec: Int64
}

class async Main {
  fn async main {
    let spec = Timespec { @tv_sec = 123 as Int64, @tv_nsec = 456 as Int64 }

    spec.tv_sec = 1000 as Int64
  }
}
```

::: warn
Inko doesn't run destructors for any types stored in a C structure, which
may lead to memory leaks if you don't manually run these where necessary.
:::

If a structure is stored in a field, referring to the field doesn't incur a
copy, instead you get a pointer to the structure. This makes it easier to work
with structures stored in Inko classes:

```inko
import std.stdio.STDOUT

class extern Timespec {
  let @tv_sec: Int64
  let @tv_nsec: Int64
}

class Box {
  let @time: Timespec
}

class async Main {
  fn async main {
    let out = STDOUT.new
    let spec = Timespec { @tv_sec = 123 as Int64, @tv_nsec = 456 as Int64 }
    let box = Box { @time = spec }

    box.time.tv_sec = 400 as Int64
    out.print((box.time.tv_sec as Int).to_string) # => 400
  }
}
```

If you want a copy anyway, you can dereference the pointer (more on that in just a
moment) to get a copy.

## Pointers

Pointer types are defined using the syntax `Pointer[T]`, where `T` is the type
pointed to. For example, the type signature for a pointer to our `Timespec` is
`Pointer[Timespec]`.

Creating a pointer to a C value is done using the `mut expr` expression, where
`expr` is an expression to create a pointer to. To illustrate this, we'll use
the libc function `clock_gettime()`, which expects a `timespec` pointer as its
second argument:

```inko
import std.stdio.STDOUT

let CLOCK_REALTIME = 0

class extern Timespec {
  let @tv_sec: Int64
  let @tv_nsec: Int64
}

fn extern clock_gettime(id: Int32, time: Pointer[Timespec]) -> Int32

class async Main {
  fn async main {
    let out = STDOUT.new
    let spec = Timespec { @tv_sec = 0 as Int64, @tv_nsec = 0 as Int64 }

    clock_gettime(CLOCK_REALTIME as Int32, mut spec)

    out.print((spec.tv_sec as Int).to_string)
  }
}
```

Here `mut spec` passes a pointer to our `Timespec` structure, allowing
`clock_gettime()` to mutate it in-place.

Note that the pointer is created to the _result_ of the `expr` expression. If
`expr` is a variable, the pointer is created to whatever value is stored in the
variable. This means that if you use `mut object.field`, and `field` returns
e.g. a structure, you create a pointer to that newly copied structure, not the
original structure stored in `field`.

### Pointers to methods

If a module method is defined using the `extern` keyword, you can create a
pointer to the method using `mut method_name`:

```inko
# This guarantees this method uses the C calling convention.
fn extern example(value: Int) -> Int {
  value
}

class async Main {
  fn async main {
    let pointer_to_method = mut example
  }
}
```

For this to work, you must leave out the arguments and parentheses in the `mut`
expression, otherwise you get a reference or pointer to the _result_ of
_calling_ the method. The use of the `extern` keyword for the method definition
is required as this guarantees the use of the C calling convention, instead of
Inko's calling convention (which may not necessarily be the same).

### Dereferencing

Dereferencing a pointer is done by reading from and writing to the pseudo field
`0`. Doing so returns a copy of the value pointed to:

```inko
import std.stdio.STDOUT

class extern Timespec {
  let @tv_sec: Int64
  let @tv_nsec: Int64
}

class async Main {
  fn async main {
    let out = STDOUT.new
    let spec = Timespec { @tv_sec = 0 as Int64, @tv_nsec = 0 as Int64 }
    let ptr = mut spec

    # This writes an entirely new value to the pointer.
    ptr.0 = Timespec { @tv_sec = 400 as Int64, @tv_nsec = 0 as Int64 }

    # This prints `tv_sec` of a _copy_ of the Timespec structure, because the
    # dereference using `ptr.0` returns a _copy_.
    out.print((ptr.0.tv_sec as Int).to_string) # => 400
  }
}
```

For pointers to structures, you can also read and write fields as with regular
Inko objects. This avoids the need for intermediate copies, allowing you to
mutate data pointed to in-place:

```inko
import std.stdio.STDOUT

class extern Timespec {
  let @tv_sec: Int64
  let @tv_nsec: Int64
}

class async Main {
  fn async main {
    let out = STDOUT.new
    let spec = Timespec { @tv_sec = 0 as Int64, @tv_nsec = 0 as Int64 }
    let ptr = mut spec

    # This modifies the data `ptr` points to, not a copy, resulting in `spec`
    # containing the new value.
    ptr.tv_sec = 400 as Int64

    out.print((ptr.tv_sec as Int).to_string)  # => 400
    out.print((spec.tv_sec as Int).to_string) # => 400
  }
}
```

::: tip
Avoid using `ptr.0.field_name` when `ptr` points to a structure, and use
`ptr.field_name` instead. This way you avoid unnecessary copies of the
structure.
:::

### Pointer arithmetic

Inko doesn't support pointer arithmetic, meaning `some_pointer + 16` is invalid.
For cases where you need to compute pointer offsets, you'll have to cast the
pointer to an `Int`, perform the computation, then cast the result back to a
pointer. For example, here we mutate `tv_nsec` using such an approach:

```inko
import std.stdio.STDOUT

class extern Timespec {
  let @tv_sec: Int64
  let @tv_nsec: Int64
}

class async Main {
  fn async main {
    let out = STDOUT.new
    let spec = Timespec { @tv_sec = 0 as Int64, @tv_nsec = 0 as Int64 }
    let ptr = mut spec

    (ptr as Int + 8 as Pointer[Int64]).0 = 400 as Int64

    out.print((spec.tv_nsec as Int).to_string)
  }
}
```

::: warn
Manually calculating pointer offsets can lead to bugs, such as reading
invalid memory. You'll want to avoid this whenever possible.
:::

## Error handling

Many C functions return some sort of flag upon encountering an error, and set
`errno` to an error code. Inko supports reading these values using
`std.io.Error.last_os_error`:

```inko
import std.stdio.STDOUT
import std.io.Error

let CLOCK_REALTIME = 0

class extern Timespec {
  let @tv_sec: Int64
  let @tv_nsec: Int64
}

fn extern clock_gettime(id: Int32, time: Pointer[Timespec]) -> Int32

class async Main {
  fn async main {
    let out = STDOUT.new
    let spec = Timespec { @tv_sec = 0 as Int64, @tv_nsec = 0 as Int64 }
    let res = clock_gettime(CLOCK_REALTIME as Int32, mut spec)
    let err = Error.last_os_error

    if res as Int == -1 { panic("clock_gettime() failed: {err}") }

    out.print((spec.tv_sec as Int).to_string)
  }
}
```

When using `Error.last_os_error`, it's crucial that you call this method
_directly_ after the C function call that may produce an error. If any code
occurs between the C function call and the `Error.last_os_error` call, the
process may be rescheduled onto a different OS thread and read the wrong value.
Further, Inko makes no attempt at clearing `errno` before C function calls, so
you should only read it when the C function call indicated some sort of value
(e.g. by returning `-1` in the above example).

In other words, code such as this **is incorrect**:

```inko
let res = clock_gettime(CLOCK_REALTIME as Int32, mut spec)

do_something_else()

let err = Error.last_os_error

if res as Int == -1 { panic("clock_gettime() failed: {err}") }
```

## Thread-local storage and the scheduler

The Inko scheduler is free to reschedule Inko processes on different OS threads.
This means that if C libraries depend on thread-local state, the state observed
may differ as a process is moved between threads. Some GUI libraries outright
break when used on a thread different from the one the thread-local state is set
up for.

The `Main` process on is guaranteed to always run on the main thread. As such,
you should only use such libraries from the `Main` process. Pinning processes to
arbitrary OS threads isn't supported.

## Runtime performance

Calling C functions comes with the same cost as when writing code in C itself.
Converting some Inko types to C types and the other way around may incur a
slight cost, but in most cases this should be negligible.

Inko's scheduler _doesn't_ detect slow/blocking C function calls, meaning it's
possible for a C function call to block the current OS thread indefinitely.

This is a deliberate choice, as detecting blocking operations incurs a runtime
cost likely too great for most cases where C libraries are necessary. In the
future we may offer a way of explicitly marking an operation as blocking,
allowing the scheduler to take care of blocking operations for you.

## Limitations

The C FFI is a bit spartan, only offering what we believe is necessary for most
of the C libraries out there. Most notably, the following isn't supported:

- Using C globals, including thread-local globals. Relying on global state is
  going to cause trouble due to Inko's concurrent nature, so even if we did
  support this it wouldn't make your life easier.
- Compiling C source code as part of the Inko build process.
- Compile-time expressions such as `sizeof()` to automatically get type sizes.
- Setting `errno` to a custom value. `errno` is implemented differently across
  libc implementations, and Rust (which we use for getting the value) doesn't
  support writing to `errno`.

[^1]: On 32-bit platforms this type would have a size of 32 bits, but Inko
  doesn't support 32-bit platforms, so in practise this value is always 64
  bits.
