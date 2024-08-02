---
{
  "title": "Cross compilation"
}
---

Inko code can be cross compiled to different targets. For example, you can
compile your code for macOS while running on Linux. Inko supports cross
compiling to [a variety of targets](#target-triples).

Cross compilation involves the following steps:

1. Decide what toolchain to use: [clang](https://clang.llvm.org/),
   [gcc](https://gcc.gnu.org/), [Zig](https://ziglang.org/), or something else.
1. Install the dependencies necessary to compile to the target platform
1. Installing the Inko runtime for the target platform
1. Compiling your code for the target platform

## Deciding on a toolchain

Inko makes use of existing technologies (such as the system linker) to compile
your code. When compiling for your current platform, objects are linked using
the `cc` executable and an optional linker can be specified (e.g.
[mold](https://github.com/rui314/mold)).

When compiling for a different platform, Inko tries to determine what executable
and arguments to use for the target platform. This of course only works if you
have the necessary executables installed, so let's get to it.

Zig greatly simplifies cross compiling code and so Inko favours using Zig over
using gcc or clang, if Zig is installed that is. For details on how to install
Zig, refer to [the Zig
documentation](https://ziglang.org/learn/getting-started/).

If Zig isn't available or desired, you can instead use gcc or clang. If both are
installed and you don't explicitly specify which one to use, Inko favours gcc
over clang.

::: tip
If you're not sure whether to use clang, gcc, Zig or something else, we highly
recommend using Zig as this makes the whole process much easier.
:::

## Installing system dependencies

If you've decided to use Zig, no additional dependencies should be necessary, as
Zig bundles everything (e.g. a C standard library) for you.

If you're using clang or gcc, you need to install a compiler toolchain for the
target platform. This is where things get difficult: some platforms provide a
package providing a toolchain for a different target platform, others don't.

### Linux to Linux

When cross compiling from Linux to Linux, you need to install the following
package(s) based on your host distribution:

|=
| Distribution
| Target
| ABI
| Package
|-
| Arch Linux
| AArch64
| GNU
| `aarch64-linux-gnu-gcc`
|-
| Arch Linux
| x86-64
| musl
| `musl`
|-
| Debian
| AArch64
| GNU
| `gcc-aarch64-linux-gnu`
|-
| Fedora
| AArch64
| GNU
| `aarch64-linux-gnu-gcc` using [this copr
  repository](https://copr.fedorainfracloud.org/coprs/lantw44/aarch64-linux-gnu-toolchain/)
|-
| Ubuntu
| AArch64
| GNU
| `gcc-aarch64-linux-gnu`
|-
| Ubuntu
| x86-64
| GNU
| `gcc-x86-64-linux-gnu`
|-
| Void
| AArch64
| GNU
| `cross-aarch64-linux-gnu`
|-
| Void
| AArch64
| musl
| `cross-aarch64-linux-musl`
|-
| Void
| x86-64
| musl
| `cross-x86_64-linux-musl`

### To and from macOS

[osxcross](https://github.com/tpoechtrager/osxcross) may prove useful when cross
compiling _to_ macOS, but the setup process is difficult and we don't have any
experience using it ourselves.

Cross compiling _from_ macOS to Linux or FreeBSD is perhaps even more difficult,
as there don't appear to be any commonly used packages to do so. Instead, it
appears the usual approach is to use a virtual machine running Linux or FreeBSD
and compile the code in the virtual machine.

Because of these complications, we _highly_ recommend using Zig when compiling
to/from macOS.

### To and from FreeBSD

Similar to compiling to macOS, Linux distributions don't provide the necessary
packages to target FreeBSD. FreeBSD in turn doesn't provide packages to compile
to Linux or macOS.

Zig should be able to cross compile from FreeBSD to Linux or macOS, but it
[doesn't support cross compiling to
FreeBSD](https://github.com/ziglang/zig/issues/2876).

## Target triples

When cross compiling, we need to specify a target triple when adding a runtime
and building our code. The following target triples are available:

|=
| OS
| Architecture
| ABI
| Triple
|-
| Linux
| x86-64
| GNU
| amd64-linux-gnu
|-
| Linux
| AArch64
| GNU
| arm64-linux-gnu
|-
| Linux
| x86-64
| musl
| amd64-linux-musl
|-
| Linux
| AArch64
| musl
| arm64-linux-musl
|-
| macOS
| x86-64
| native
| amd64-mac-native
|-
| macOS
| AArch64
| native
| arm64-mac-native
|-
| FreeBSD
| x86-64
| native
| amd64-freebsd-native

## Installing the runtime

Inko uses a small runtime library written in Rust, used for scheduling
processes, allocating memory, and more. When cross compiling, you'll need to
ensure a runtime for the target is installed. A runtime is installed using the
command `inko runtime add TARGET`, where `TARGET` is one of the target triples
listed in the above table.

Runtimes are removed using `inko runtime remove TARGET`, and you can list the
available and installed runtimes using `inko runtime list`. For more
information, run `inko runtime --help`.

As an example, to install the runtime for compiling to x86-64 macOS, run the
following:

```
inko runtime add amd64-mac-native
```

## Cross-compiling your code

With the system dependencies and the runtime installed, we can start cross
compiling our code. We'll cross compile the following program located in the
file `test.inko`:

```inko
import std.stdio (STDOUT)

class async Main {
  fn async main {
    STDOUT.new.print('hello')
  }
}
```

To build this for AArch64 Linux, run the following:

```bash
inko runtime add arm64-linux-gnu # If not done already
inko build --target=arm64-linux-gnu test.inko
```

If all went well, the resulting `test` executable is located at
`./build/arm64-linux-gnu/test`.

If Zig is installed, we can also cross compile to macOS without having to
install anything extra:

```bash
inko runtime add amd64-mac-native
inko build --target=amd64-mac-native test.inko
```

### Using a custom linker

Inko tries to detect what linker to use based on the compilation target.
Depending on the target you're compiling to, you may need to manually specify
the linker to use. This can be done using the `--linker` and `--linker-arg`
options. For example, if we want to explicitly use `aarch64-linux-gnu-gcc` we
can do so as follows:

```bash
inko build --target=arm64-linux-gnu --linker=aarch64-linux-gnu-gcc test.inko
```

The `--linker-arg` option is used to pass extra options to the linker:

```bash
inko build --target=arm64-linux-gnu \
  --linker=clang \
  --linker-arg='--sysroot=/usr/aarch64-linux-gnu' \
  --linker-arg='--target=aarch64-linux-gnu' \
  test.inko
```

Here we've used clang as the linker, and used the `--linker-arg` options to
specify the toolchain location and the target to compile to.

In general you shouldn't need to manually specify the linker or extra linker
arguments.

### Using LLD or musl

Inko supports linking using LLD or musl instead of the system linker. When
cross compiling with the options `--linker=lld` or `--linker=musl`, the `inko
build` command may override the linker depending on the target the code is
compiled for. For example, when using gcc for cross compilation the linker is
always set to the system linker to reduce the chances of running into any linker
related errors (e.g. some linkers don't support certain AArch64 CPUs).

## Using C libraries

Using C libraries can greatly complicate cross compilation, as you'll have to
install the library for each target you wish to compile to. This likely involves
a lot of manual work, such as compiling the libraries from source and placing
them in the right directory. For this reason (along with the lack of safety that
comes with using C libraries) we recommend you avoid using C libraries as much
as possible.

If you _have_ to use C libraries, it's best to compile your code in a virtual
machine or container of sorts. Inko doesn't provide anything to make this
easier, and likely won't for the foreseeable future, if ever.
