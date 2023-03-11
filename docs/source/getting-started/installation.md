# Installation

Inko's virtual machine and compiler are written in
[Rust](https://www.rust-lang.org/), bundled into a single executable compiled
using Rust's "cargo" package manager/build tool.

Inko officially supports Linux, macOS, and Windows. BSDs and other Unix-like
operating systems should also work, but are not officially supported at this
time.

Windows users can build Inko using the Visual Studio build tools, or using a
Unix compatibility layer such as [MSYS2][msys2].

## Requirements

- A 64-bits little-endian platform
- A CPU with AES-NI support
- Rust 1.62 or newer

Inko's package manager (ipm) also required Git to be installed, and the `git`
executable to be available in your PATH.

For Unix based platforms, the following must also be available:

- Make
- sh, bash or a compatible shell
- A C compiler such as GCC or clang

These dependencies are not needed when building for Windows when using the
Visual studio build tools. They _are_ needed when building under MSYS2 or
similar Unix compatibility layers.

## Installing

### Cross-platform

The easiest way to install Inko is to use Inko's own version manager:
[ivm][ivm]. ivm supports all the platforms officially supported by Inko,
including Windows. For more information on how to install and use ivm, refer to
the [ivm guide][ivm].

Once installed, you can install Inko as follows:

```bash
ivm install latest # Installs the latest version of Inko
ivm install 0.10.0 # Installs version 0.10.0
```

### Docker

If you are using [Docker](https://www.docker.com/) or
[Podman](https://podman.io/), you can use our official Docker images. These
images are published on Docker Hub in the [inkolang/inko
repository](https://hub.docker.com/r/inkolang/inko).

To install Inko 0.10.0, run the following:

=== "Docker"
    ```bash
    docker pull inkolang/inko:0.10.0
    ```
=== "Podman"
    ```bash
    podman pull inkolang/inko:0.10.0
    ```

You can then run Inko as follows:

=== "Docker"
    ```bash
    docker run inkolang/inko:0.10.0 inko --version
    ```
=== "Podman"
    ```bash
    podman run inkolang/inko:0.10.0 inko --version
    ```

A full list of all available tags [is found
here](https://hub.docker.com/r/inkolang/inko/tags).

### Arch Linux

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

### macOS

Inko is available in [Homebrew](https://brew.sh/):

```bash
brew install inko
```

The Homebrew formula is maintained by Homebrew and its contributors. For
issues specific to the formula (e.g. it doesn't work on a certain version of
macOS), please report issues in the [homebrew-core issue
tracker](https://github.com/Homebrew/homebrew-core/issues).

### From source

When building from Git, first clone the repository:

```bash
git clone https://gitlab.com/inko-lang/inko.git
cd inko
```

Or use a release tarball:

```bash
mkdir 0.10.0
curl https://releases.inko-lang.org/0.10.0.tar.gz -o 0.10.0.tar.gz
tar -C 0.10.0 -xf 0.10.0.tar.gz
cd 0.10.0
```

To compile a development build, run `cargo build`. For a release build,
run `cargo build --release` instead. After building you can find the `inko`
executable in `target/release/inko` (or `target/debug/inko` for a debug build),
and the `ipm` executable in `target/release/ipm` (or `target/debug/ipm` for
debug builds).

By default Inko uses the standard library provided in the Git repository,
located in `libstd/src`. If you wish to use a different directory, set the
`INKO_LIBSTD` environment variable to a path of your choosing. For example:

```bash
INKO_LIBSTD=/tmp/libstd/src cargo build --release
```

This builds Inko such that it uses the standard library located at
`/tmp/libstd/src`.

When building from source you can set certain feature flags to customise the
installation. These flags are specified like so:

```bash
cargo build --release --features foo,bar
```

The following feature flags are available:

| Feature flag  | Default  | Description
|:--------------|:---------|:--------------
| libffi-system | Disabled | Dynamically link against [libffi][libffi], instead of compiling it from source.
| jemalloc      | Disabled | Use [jemalloc][jemalloc] instead of the system allocator.

## Packaging

To ease the process of building a package of Inko, consider using the Makefile
provided as part of each release. Using this Makefile, the process (at least in
most cases) is as simple as running the following:

```bash
make build PREFIX=/usr
make install PREFIX=/usr DESTDIR=./chroot
```

The `PREFIX` variable specifies the base path of all files to install, while
`DESTDIR` specifies a directory to move the files into.

The `PREFIX` variable must be specified for both `make build` and
`make install`. The `DESTDIR` variable defaults to the value of the `PREFIX`
variable.

When packaging Inko it's best to use a system wide installation of FFI, instead
of building it from source when compiling Inko. To do so, build Inko as follows:

```bash
make build FEATURES=libffi-system
```

Or if you don't want to use make:

```bash
cargo build --release --features libffi-system
```

[ivm]: ivm.md
[homebrew]: https://brew.sh/
[msys2]: http://www.msys2.org/
[libffi]: https://sourceware.org/libffi/
[jemalloc]: http://jemalloc.net/
