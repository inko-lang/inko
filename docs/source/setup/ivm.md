---
{
  "title": "Using Inko's version manager"
}
---

[ivm](https://github.com/inko-lang/ivm) is a tool used to install and manage
different versions of Inko, independent from your system's package manager. ivm
is written in Rust.

## Requirements

- Rust 1.78 or newer
- The [dependencies](../installation#dependencies) necessary to build Inko from
  source

## Installing

### Arch Linux

```bash
yay -S ivm
```

### Fedora

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

::: note
If a package is available for your platform, we recommend installing ivm
through your platform's package manager instead. Once ivm is available on
enough platforms, we may stop publishing it to crates.io.
:::

```bash
cargo install ivm
```

This installs the `ivm` executable in `$HOME/.cargo/bin`, where `$HOME` is your
home directory. You need to add this to your shell's PATH if not done already:

```bash
export PATH="$HOME/.cargo/bin:$PATH"  # Using Bash
fish_add_path --path $HOME/.cargo/bin # Using Fish
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

::: tip
Make sure to set a default version after installing Inko, otherwise you have
to use `ivm run VERSION inko ...` to use Inko.
:::

::: note
Make sure the [dependencies](../installation#dependencies) necessary for your
platform are installed, and that any required environment variables are set
_before_ running `ivm install`.
:::

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
