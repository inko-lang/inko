# Installation

Inko's native code compiler is written in [Rust](https://www.rust-lang.org/) and
uses [LLVM](https://llvm.org/) as its backend. The generated machine code links
against a small runtime library, also written in Rust.

This guide covers the steps needed to get Inko installed on your platform of
choice.

## Supported platforms

Inko supports macOS and Linux. Inko _should_ also work on the various BSDs (e.g.
FreeBSD), but at the moment we don't actively test on these platforms.

Inko historically also supported Windows, but we dropped support with the
introduction of the native code compiler. Our knowledge of Windows is limited,
and the cost of maintaining Windows support isn't worth it. In the future we may
support Windows again, providing somebody is willing to maintain the necessary
changes.

## Requirements

- A 64-bits little-endian platform
- Rust 1.63 or newer
- LLVM 15, with support for static linking against LLVM
- A C compiler such as [GCC](https://gcc.gnu.org/) or
  [clang](https://clang.llvm.org/)
- Git, for managing Inko packages

[lld](https://lld.llvm.org/) is an optional dependency of Inko, and is used
automatically on Linux when available.

## Installing

### Cross-platform

The easiest way to install Inko is to use Inko's own version manager:
[ivm](ivm.md). ivm supports all the platforms officially supported by Inko.

!!! info
    Don't forget to install the necessary dependencies using your system's
    package manager, as ivm doesn't do this for you. You can find the list of
    the necessary packages to install below.

Once ivm is installed installed, you can install Inko as follows:

```bash
ivm install latest
```

This installs the latest known version. If you want to install a specific
version, run the following instead (where `X.Y.Z` is the version you want to
install):

```bash
ivm install X.Y.Z
```

For more details on how to use ivm and switch versions, refer to the [ivm
guide](ivm.md).

### From source

When building from Git, first clone the repository:

```bash
git clone https://github.com/inko-lang/inko.git
cd inko
```

Or use a release tarball:

```bash
mkdir 0.11.0
curl https://releases.inko-lang.org/0.11.0.tar.gz -o 0.11.0.tar.gz
tar -C 0.11.0 -xf 0.11.0.tar.gz
cd 0.11.0
```

You can then compile Inko as follows:

| Mode    | Command                 | Executable              | Runtime library
|:--------|:------------------------|:------------------------|:-----------------------
| Debug   | `cargo build`           | `./target/debug/inko`   | `./target/debug/libinko.a`
| Release | `cargo build --release` | `./target/release/inko` | `./target/release/libinko.a`

In both cases the standard library in `std/src` is used. You can customise the
standard library and runtime library paths by setting these environment
variables when running `cargo` build:

- `INKO_STD`: the full path to the directory containing the standard library
  modules, defaults to `./std/src`.
- `INKO_RT`: the full path to the directory containing the runtime libraries to
  link the generated code against, defaults to `./target/MODE` where `MODE` is
  either `debug` for debug builds or `release` for release builds.

If you are building a package, it's recommended to use the provide `Makefile`
instead, as this simplifies the process of moving the necessary files in place
and using the right paths. To compile a release build of Inko, run `make` and
`make install` to install the files. This process can be customised by setting
the following Make variables:

- `DESTDIR`: the directory to install files into when running `make install`.
- `PREFIX`: the path prefix to use for all files, defaults to `/usr`. When
  combined with `DESTDIR`, the value of `DESTDIR` prefixes this value.

For example:

```bash
make PREFIX=/usr/local
make install DESTDIR=./package-root PREFIX=/usr/local
```

The `PREFIX` variable must be set for both the `make` and `make install`
commands, but `DESTDIR` is only necessary for `make install`.

### Linux

Dependencies are split into two categories: the dependencies of the compiler,
and the dependencies of the produced executable. The compiler dependencies only
need to be installed in your development environment.

#### Alpine

!!! warning
    Due to [this bug](https://gitlab.com/taricorp/llvm-sys.rs/-/issues/44) in
    the llvm-sys crate, compiling the compiler for musl targets (which includes
    Alpine) fails with the error "could not find native static library `rt`,
    perhaps an -L flag is missing?".

There's no official package for Inko in the Alpine repositories.

When building from source, the compiler requires the following dependencies to
be installed:

```bash
sudo apk add build-base rust cargo llvm15 llvm15-dev llvm15-static git
```

The generated code requires the following dependencies:

```bash
sudo apk add libgcc
```

#### Arch

Two AUR packages are provided: `inko` and `inko-git`. These can be installed
using your favourite AUR wrapper:

=== "yay"
    ```bash
    yay -S inko
    ```
=== "pacaur"
    ```bash
    pacaur -S inko
    ```
=== "pikaur"
    ```bash
    pikaur -S inko
    ```
=== "Manually"
    ```bash
    git clone https://aur.archlinux.org/inko.git
    cd inko
    makepkg -si
    ```

When building from source, the compiler requires the following dependencies to
be installed:

```bash
sudo pacman -Sy llvm rust git base-devel
```

The generated code requires the following dependencies:

```bash
sudo pacman -Sy gcc-libs
```

#### Fedora

Inko isn't included in the Fedora repositories, nor is there a
[copr](https://copr.fedorainfracloud.org/coprs/) package (though [this is
planned](https://github.com/inko-lang/inko/issues/364)).

When building from source, the compiler requires the following dependencies to
be installed:

```bash
sudo dnf install gcc make rust cargo llvm15 llvm15-devel llvm15-static libstdc++-devel libstdc++-static libffi-devel zlib-devel git
```

The generated code requires the following dependencies:

```bash
sudo dnf install libgcc
```

#### Ubuntu

There's no official package for Inko in the Ubuntu repositories.

When building from source, the compiler requires the following dependencies to
be installed:

```bash
sudo apt-get install --yes rustc cargo git build-essential llvm-15 llvm-15-dev libstdc++-11-dev libclang-common-15-dev zlib1g-dev
```

The generated code requires the following dependencies:

```bash
sudo dnf install libgcc-s1
```

### macOS

Inko is available in [Homebrew](https://brew.sh/):

```bash
brew install inko
```

The Homebrew formula is maintained by Homebrew and its contributors. For
issues specific to the formula (e.g. it doesn't work on a certain version of
macOS), please report issues in the [homebrew-core issue
tracker](https://github.com/Homebrew/homebrew-core/issues).

To build from source, install the necessary dependencies as follows:

```bash
brew install llvm@15 rust git
```

When building from source, you may need to add the LLVM `bin` directory to your
`PATH` as follows:

```bash
export PATH="$(brew --prefix llvm@15)/bin:$PATH"
```

You may also need to set the `LIBRARY_PATH` to the LLVM `lib` directory, though
this doesn't always appear to be necessary:

```bash
export LIBRARY_PATH="$(brew --prefix llvm@15)/lib"
```

### Docker

If you are using [Docker](https://www.docker.com/) or
[Podman](https://podman.io/), you can use our official Docker/Podman images.
These images are published on
[GitHub.com](https://github.com/inko-lang/inko/pkgs/container/inko).

To install a specific version, run the following (replacing `X.Y.Z` with the
version you want to install):

=== "Docker"
    ```bash
    docker pull ghcr.io/inko-lang/inko:X.Y.Z
    ```
=== "Podman"
    ```bash
    podman pull ghcr.io/inko-lang/inko:X.Y.Z
    ```

You can then run Inko as follows:

=== "Docker"
    ```bash
    docker run inko-lang/inko:X.Y.Z inko --version
    ```
=== "Podman"
    ```bash
    podman run inko-lang/inko:X.Y.Z inko --version
    ```

We also build a container for every commit on the `main` branch, provided the
tests are passing. If you like to live dangerously, you can use these as
follows:

=== "Docker"
    ```bash
    docker pull ghcr.io/inko-lang/inko:main
    docker run inko-lang/inko:main inko --version
    ```
=== "Podman"
    ```bash
    podman pull ghcr.io/inko-lang/inko:main
    podman run inko-lang/inko:main inko --version
    ```
