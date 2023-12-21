# The Inko version manager

Inko has its own version manager: [ivm](https://github.com/inko-lang/ivm).
Using ivm you can install and manage multiple versions of Inko. ivm is written
in Rust.

## Installing

ivm itself only requires Rust 1.68 or newer, but to build Inko itself you'll
need to also meet the requirements listed in the [installation
guide](installation.md).

### Arch Linux

ivm can be installed using the AUR using an AUR wrapper of your choice. For
example, using [yay](https://github.com/Jguer/yay):

```bash
yay -S ivm
```

### Fedora

Inko's [Copr repository](https://copr.fedorainfracloud.org/coprs/yorickpeterse/inko/)
can be used to install ivm as follows:

```bash
sudo dnf install dnf-plugins-core
sudo dnf copr enable yorickpeterse/inko
sudo dnf install ivm
```

### From source

Clone the repository:

```bash
git clone https://github.com/inko-lang/ivm.git
cd ivm
cargo build --release
```

The resulting executable is found in `target/release/ivm`.

### Using crates.io

!!! note
    If a package is available for your platform, we recommend installing ivm
    through your platform's package manager instead. Once ivm is available on
    enough platforms, we may stop publishing it to crates.io.

ivm is available on [crates.io](https://crates.io/), and you can install it as
follows:

```bash
cargo install ivm
```

This installs the `ivm` executable in `$HOME/.cargo/bin`, where `$HOME` is your
home directory. You need to add this to your shell's PATH if not done already:

=== "Bash"
    ```bash
    export PATH="$HOME/.cargo/bin:$PATH"
    ```
=== "Fish"
    ```bash
    fish_add_path --path $HOME/.cargo/bin
    ```

For more information, refer to [this rustup documentation
page](https://rust-lang.github.io/rustup/installation/index.html).

To update ivm, run the following:

```bash
cargo install ivm --force
```

## Setting up your PATH

Once ivm is installed, you need to add its bin path to your `PATH` variable.
This is needed to ensure that executables such as `inko` are available. To add
the path, run `ivm show bin`, then add the path it prints out to your `PATH`
variable. For example:

```bash
$ ivm show bin
/var/home/yorickpeterse/homes/fedora/.local/share/ivm/bin
```

Assuming you're using Bash as your shell, you'd add the following to your
`.bashrc`:

```bash
export PATH="$HOME/.local/share/ivm/bin:$PATH"
```

## Usage

To install a version (e.g. 0.10.0):

```bash
ivm install 0.10.0    # This will install version 0.10.0
ivm install latest    # This will install the latest available version
```

!!! tip
    Make sure to set a default version after installing Inko, otherwise you have
    to use `ivm run VERSION inko ...` to use Inko.

To remove a version:

```bash
ivm remove 0.10.0    # This will remove version 0.10.0
ivm remove latest    # This will remove the latest _installed_ version
```

To list all installed versions:

```bash
ivm list
```

To list all available versions:

```bash
ivm known
```

To change the default Inko version:

```bash
ivm default 0.10.0
```

To remove any temporary data:

```bash
ivm clean
```

To run a command with a specific Inko version:

```bash
ivm run 0.10.0 inko --version # This will run `inko --version` using Inko 0.10.0
ivm run latest inko
```

To remove all data of ivm (except the ivm executable itself):

```bash
ivm implode
```

For more information, run `ivm --help`.

## Setting a default version

The `default` command is used to set a default Inko version to use. When set,
ivm will create a symbolic link in its `bin/` directory to the `inko` executable
of the default version. By setting a default version you can just use `inko ...`
instead of the much more verbose `ivm run VERSION inko ...`.

## Packaging ivm

If you are building a package of ivm (e.g. for Debian), you can use the provided
`Makefile` instead of `cargo build`:

```bash
make
make install
```

This process can be customised by setting the following Make variables:

- `DESTDIR`: the directory to install files into when running `make install`.
- `PREFIX`: the path prefix to use for all files, defaults to `/usr`. When
  combined with `DESTDIR`, the value of `DESTDIR` prefixes this value.

