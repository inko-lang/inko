---
{
  "title": "Goals and non-goals"
}
---

Inko has a set of goals it wants to achieve, and certain things we explicitly
don't want to implement/provide, which we list below.

::: note
This is a list of _goals_, and as such some goals have yet to be
implemented.
:::

## Goals

### Batteries included

For the standard library we're taking a "batteries included" approach, providing
a wide range of functionality as part of the standard library, reducing the
amount of third-party packages needed.

### An easy to understand type system

While a sophisticated type system may allow for more checks to be done at
compile-time instead of runtime, it also increases the complexity. Inko instead
aims to provide a type system that balances ease of use, correctness, and
compile-time performance, instead of focusing entirely on (for example)
correctness.

As an example, support for (generic) associated types and higher-kinded types is
an explicit non-goal for Inko.

### A balance between compile-time and runtime performance

Instead of favouring runtime performance over compile-time performance, Inko
tries to provide a good balance between the two. For improved runtime
performance the compiler supports the option to enable more aggressive
optimisations at the cost of compile times, should you truly need this.

### Memory safety, without the mental overhead

Inko programs should be memory safe, but without the compile-time complexity
associated with other languages (e.g. Rust). To achieve this, Inko performs some
work at runtime to ensure your program is correct (on top of the work done at
compile-time of course). This means Inko might not be suitable for all types of
applications, but it should make it much easier to work with Inko.

### Easy cross-compilation

Cross-compiling your Inko program should be easy, and not require additional
software aside from the absolute essentials (e.g. a linker supporting the target
platform).

### Use Inko as much as possible

Drawing inspiration from Java, [Rubinius](https://github.com/rubinius/rubinius)
and Smalltalk, we aim to write as much code in Inko as possible, instead of
glueing together a collection of C libraries. This helps us achieve some of our
other goals, such as easier cross-compilation and memory safety, and makes
debugging Inko programs easier as you only have to debug code written in one
language.

Sometimes you just have to use a C library and that's OK, but it shouldn't be
the default approach.

### A simple package manager

Inko's package manager aims to be simple and easy to use. To achieve this, it
won't support features found elsewhere such as complex version requirements. In
addition, it will only support [minimal version
selection](https://research.swtch.com/vgo-mvs) rather than using a complex SAT
solver.

### Concurrency that's easy to use

We want it to be easy to write concurrent programs, but without the
complexity/mess of contemporary solutions such as async/await. For example,
[function colouring](http://journal.stuffwithstuff.com/2015/02/01/what-color-is-your-function/)
isn't a thing in Inko, and unlike [Go](https://go.dev/), Inko's approach to
concurrency makes race conditions impossible.

### Simple and stable syntax

Inko's syntax aims to be simple to understand by both humans and computers. In
general we prefer to implement features using regular types and methods, rather
than adding new syntax.

Once Inko reaches version 1.0.0, the goal is to freeze the syntax for the
foreseeable future. This makes it easier to write tools such as code formatters
and linters, as syntax changes would be rare.

### Fewer settings and better defaults

Instead of providing dozens of settings to tweak the behaviour of Inko programs,
we aim to provide defaults that are good enough for 95% of all use cases. For
the remaining 5% we'd prefer to first develop a better understanding of the use
cases and gather feedback from the community, before adding a new setting of
sorts.

## Non-goals

### Compiling C code when installing a package

Inko's package manager focuses on Inko source code, and will not include the
ability to compile C source code (or any other language for that matter) into a
library as part of a package's installation process. Not only would this slow
down installing of packages, it also poses a significant security risk, and
complicates the process of distributing Inko code.

Instead, we believe it's best to avoid using C as much as possible, and only use
pre-installed libraries (using your system's package manager) when you have no
other option.

### Compile-time code execution

Compile-time function execution, macros and related features are explicitly
_not_ desired. While such features can be useful, they complicate the language
and compiler, can result in a significant increase of compilation timings, and
can make debugging a lot more difficult.

Instead, we believe generating code ahead of time is a better solution. While
such an approach incurs a cost, this cost is only relevant when generating the
code.

### Using Inko from other languages

Inko's runtime makes it difficult to expose Inko programs through a C interface,
thus making it difficult to use Inko from other languages. Even if this wasn't
difficult, it's not something we're interested in as we'd rather _replace_ those
languages with Inko, at least as much as possible.

### Low-level memory operations

Inko is explicitly _not_ a systems language, and this means it will never
(publicly) support low-level memory operations, such as raw memory allocations,
pointers, custom allocators, and other features typically found in systems
languages.

### A central package database

Hosting a central package database such as [RubyGems](https://rubygems.org/) is
incredibly costly, and we don't have the resources to provide such a service.
Instead, Inko's package manager is decentralised. To make finding Inko packages
easier, we'll provide a package _index_ at some point in the future.

### Supporting every platform under the sun

Many operating systems/platforms exist today, but supporting all them is
explicitly _not_ a goal. Instead, Inko focuses on mainstream platforms such as
Linux and the various BSDs. This means we probably won't accept any patches that
add support for e.g. [Redox](https://www.redox-os.org/) until it becomes more
commonly used.

Embedded platforms are not a goal for the time being, but might be in the
future.

### Pinning processes to OS threads

Inko guarantees that the `Main` process is always run on the main thread, but
beyond that doesn't support pinning of processes to OS threads. This is by
design as support for such a feature incurs additional scheduler complexity and
overhead, and may result in processes not running when too many other processes
are pinned to the available threads.
