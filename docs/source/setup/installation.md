---
{
  "title": "Installation"
}
---

Inko's native code compiler is written in [Rust](https://www.rust-lang.org/) and
uses [LLVM](https://llvm.org/) as its backend. The generated machine code links
against a small runtime library, also written in Rust.

This guide covers the steps needed to get Inko installed on your platform of
choice.

## Supported platforms

Inko supports Linux, macOS, and FreeBSD (13.2 or newer). Inko might also work on
other platforms, but we only provide support for the listed platforms. Windows
isn't supported.

## Requirements

- A 64-bits little-endian platform
- Rust 1.70 or newer
- LLVM 15
- A C compiler such as [GCC](https://gcc.gnu.org/) or
  [clang](https://clang.llvm.org/)
- Git, for managing packages using the `inko pkg` command

## Cross-platform

The easiest way to install Inko is to use Inko's own version manager:
[ivm](ivm). ivm supports all the platforms officially supported by Inko.

When installing Inko using ivm, you must first install the dependencies listed
[here](#dependencies).

Once ivm is installed, you can install Inko as follows:

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
guide](ivm).

## Docker

If you are using [Docker](https://www.docker.com/) or
[Podman](https://podman.io/), you can use our official Docker/Podman images.
These images are published on
[GitHub.com](https://github.com/inko-lang/inko/pkgs/container/inko).

To install a specific version, run the following (replacing `X.Y.Z` with the
version you want to install):

```bash
docker pull ghcr.io/inko-lang/inko:X.Y.Z # Using Docker
podman pull ghcr.io/inko-lang/inko:X.Y.Z # Using Podman
```

You can then run Inko as follows:

```bash
docker run inko-lang/inko:X.Y.Z inko --version # Using Docker
podman run inko-lang/inko:X.Y.Z inko --version # Using Podman
```

We also build a container for every commit on the `main` branch, provided the
tests are passing. If you like to live dangerously, you can use these as
follows:

```bash
# Using Docker:
docker pull ghcr.io/inko-lang/inko:main
docker run inko-lang/inko:main inko --version

# Using Podman:
podman pull ghcr.io/inko-lang/inko:main
podman run inko-lang/inko:main inko --version
```

## Arch Linux

Inko is available in the [AUR](https://aur.archlinux.org/):

```bash
yay -S inko     # Latest stable release
yay -S inko-git # Latest Git commit
```

## Fedora

Inko is available as a
[Copr](https://copr.fedorainfracloud.org/coprs/yorickpeterse/inko/) repository:

```bash
sudo dnf install dnf-plugins-core
sudo dnf copr enable yorickpeterse/inko
sudo dnf install inko
```

## FreeBSD

Inko is available [as a port](https://www.freshports.org/lang/inko):

```bash
sudo pkg install inko
```

## macOS

Inko is available in [Homebrew](https://brew.sh/):

```bash
brew install inko
```

You may need to add the LLVM `bin` directory to your `PATH` as follows:

```bash
export PATH="$(brew --prefix llvm@15)/bin:$PATH"
```

You may also need to set the `LIBRARY_PATH` to the following, though this
doesn't appear to always be needed:

```bash
export LIBRARY_PATH="$(brew --prefix llvm@15)/lib:$(brew --prefix zstd)/lib"
```

## From source

When building from Git, first clone the repository:

```bash
git clone https://github.com/inko-lang/inko.git
cd inko
```

Or use a release tarball:

```bash
VER='0.13.2' # Replace this with the latest version of Inko

mkdir $VER
curl https://releases.inko-lang.org/$VER.tar.gz -o $VER.tar.gz
tar -C $VER -xf $VER.tar.gz
cd $VER
```

You can then compile Inko as follows:

|=
| Mode
| Command
| Executable
| Runtime library
|-
| Debug
| `cargo build`
| `./target/debug/inko`
| `./target/debug/libinko.a`
|-
| Release
| `cargo build --release`
| `./target/release/inko`
| `./target/release/libinko.a`

In both cases the standard library in `std/src` is used. You can customise the
standard library and runtime library paths by setting these environment
variables when running `cargo` build:

- `INKO_STD`: the full path to the directory containing the standard library
  modules, defaults to `./std/src`.
- `INKO_RT`: the full path to the directory containing the runtime libraries to
  link the generated code against, defaults to `./target/MODE` where `MODE` is
  either `debug` for debug builds or `release` for release builds.

If you are building a package, it's recommended to use the provided `Makefile`
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

### FreeBSD

When building from source, you may have to set the `LIBRARY_PATH` variable as
follows:

```bash
LIBRARY_PATH="/usr/local/lib" cargo build
```

Without this the linker may fail to find the zstd and libffi libraries, which
are needed by LLVM on FreeBSD.

## Dependencies

When building from source or using [ivm](ivm), you'll first need to install
the compiler's dependencies.

### Arch Linux

```bash
sudo pacman -Sy llvm15 rust git base-devel
```

### Alpine

::: warn
Due to [this bug](https://gitlab.com/taricorp/llvm-sys.rs/-/issues/44) in
the llvm-sys crate, compiling the compiler for musl targets (which includes
Alpine) fails with the error "could not find native static library `rt`,
perhaps an -L flag is missing?".
:::

```bash
sudo apk add build-base rust cargo llvm15 llvm15-dev llvm15-static git
```

### Debian

For Debian 12:

```bash
sudo apt-get install --yes rustc cargo git build-essential llvm-15 llvm-15-dev \
    libstdc++-11-dev libclang-common-15-dev zlib1g-dev
```

For Debian 11:

```bash
curl https://apt.llvm.org/llvm-snapshot.gpg.key | \
    sudo tee /etc/apt/trusted.gpg.d/apt.llvm.org.asc
sudo add-apt-repository \
    "deb http://apt.llvm.org/bullseye/ llvm-toolchain-bullseye-15 main"
sudo apt-get update
sudo apt-get install --yes git build-essential llvm-15 llvm-15-dev \
    libstdc++-10-dev libclang-common-15-dev zlib1g-dev libpolly-15-dev
```

The version of Rust provided by Debian 11 is too old, so you'll need to use
[rustup](https://rustup.rs/) to install the required Rust version.

### Fedora

For version 38 and newer:

```bash
sudo dnf install gcc make rust cargo llvm15 llvm15-devel llvm15-static \
    libstdc++-devel libstdc++-static libffi-devel zlib-devel git
```

For version 37:

```bash
sudo dnf install gcc make rust cargo llvm llvm-devel llvm-static \
    libstdc++-devel libstdc++-static libffi-devel zlib-devel git
```

### FreeBSD

```bash
sudo pkg install llvm15 rust git
```

### macOS

```bash
brew install llvm@15 rust git
```

### Ubuntu

For Ubuntu 22.04:

```bash
sudo apt-get install --yes rustc cargo git build-essential llvm-15 llvm-15-dev \
    libstdc++-11-dev libclang-common-15-dev zlib1g-dev
```

For Ubuntu 20.04:

```bash
curl https://apt.llvm.org/llvm-snapshot.gpg.key | \
    sudo tee /etc/apt/trusted.gpg.d/apt.llvm.org.asc
sudo add-apt-repository \
    "deb http://apt.llvm.org/focal/ llvm-toolchain-focal-15 main"
sudo apt-get update
sudo apt-get install --yes git build-essential llvm-15 llvm-15-dev \
    libstdc++-10-dev libclang-common-15-dev zlib1g-dev libpolly-15-dev
```

The version of Rust provided by 20.04 is too old, so you'll need to use
[rustup](https://rustup.rs/) to install the required Rust version.
