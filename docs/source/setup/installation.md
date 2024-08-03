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
- Rust 1.78 or newer
- LLVM 17 or newer
- A C compiler such as [GCC](https://gcc.gnu.org/) or
  [clang](https://clang.llvm.org/)
- Git, for managing packages using the `inko pkg` command

::: note
While using newer versions of LLVM may work, it's possible for a newer version
to introduce breaking changes. As such, we recommend only using a newer version
of LLVM if your platform doesn't offer a package for the version listed above.
:::

## Cross-platform

The easiest way to install Inko is to use Inko's own version manager:
[ivm](ivm). ivm supports all the platforms officially supported by Inko.

When installing Inko using ivm, you must first install the dependencies listed
[here](#dependencies).

Once ivm is installed, you can install Inko as follows:

```bash
ivm install latest
```

::: note
ivm installs Inko from source, so you'll need to install the necessary
[dependencies](#dependencies) for your platform first.
:::

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

## From source

When building from Git, first clone the repository:

```bash
git clone https://github.com/inko-lang/inko.git
cd inko
```

Or use a release tarball:

```bash
VER='0.15.0' # Replace this with the latest version of Inko

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

## Dependencies

When building from source or using [ivm](ivm), you'll need to install the
necessary dependencies.

### Arch Linux

```bash
sudo pacman -Sy llvm rust git base-devel
```

### Alpine

::: warn
Due to [this bug](https://gitlab.com/taricorp/llvm-sys.rs/-/issues/44) in
the llvm-sys crate, compiling the compiler for musl targets (which includes
Alpine) fails with the error "could not find native static library `rt`,
perhaps an -L flag is missing?".
:::

```bash
sudo apk add build-base rust cargo llvm17 llvm17-dev llvm17-static git
```

### Debian

Debian 13:

```bash
sudo apt-get install --yes rust cargo git build-essential llvm-17 llvm-17-dev libstdc++-11-dev libclang-common-17-dev zlib1g-dev libzstd-dev
```

Debian 12:

```bash
curl https://apt.llvm.org/llvm-snapshot.gpg.key | sudo tee /etc/apt/trusted.gpg.d/apt.llvm.org.asc
sudo add-apt-repository "deb http://apt.llvm.org/bookworm/ llvm-toolchain-bookworm-17 main"
sudo apt-get update
sudo apt-get install --yes git build-essential llvm-17 llvm-17-dev libstdc++-10-dev libclang-common-17-dev zlib1g-dev libpolly-17-dev libzstd-dev
```

For older versions, refer to [LLVM's Debian/Ubuntu packages page][llvm-apt] and
adjust the `add-apt-repository` accordingly.

::: note
For Debian 12 and older, the version of Rust is too old, so you'll need to use
[rustup](https://rustup.rs/) to install Rust.
:::

### Fedora

For Fedora 40 and newer:

```bash
sudo dnf install gcc make rust cargo llvm17 llvm17-devel llvm17-static libstdc++-devel libstdc++-static libffi-devel zlib-devel git
```

For Fedora 39:

```bash
sudo dnf install gcc make rust cargo llvm llvm-devel llvm-static libstdc++-devel libstdc++-static libffi-devel zlib-devel git
```

Older versions of Fedora aren't supported.

### FreeBSD

```bash
sudo pkg install llvm17 rust git
```

### macOS

```bash
brew install llvm@17 rust git
```

### Ubuntu

For Ubuntu 24.04 and newer:

```bash
sudo apt-get install --yes rustc cargo git build-essential llvm-17 llvm-17-dev libstdc++-11-dev libclang-common-17-dev zlib1g-dev libzstd-dev
```

For 23.10:

```bash
curl https://apt.llvm.org/llvm-snapshot.gpg.key | sudo tee /etc/apt/trusted.gpg.d/apt.llvm.org.asc
sudo add-apt-repository "deb http://apt.llvm.org/mantic/ llvm-toolchain-mantic-17 main"
sudo apt-get update
sudo apt-get install --yes rustc cargo git build-essential llvm-17 llvm-17-dev libstdc++-10-dev libclang-common-17-dev zlib1g-dev libpolly-17-dev libzstd-dev
```

For older versions, refer to [LLVM's Debian/Ubuntu packages page][llvm-apt] and
adjust the `add-apt-repository` accordingly.

[llvm-apt]: https://apt.llvm.org/
