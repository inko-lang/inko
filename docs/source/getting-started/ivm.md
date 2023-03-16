# Using Inko's version manager

Inko has its own version manager: [ivm](https://github.com/inko-lang/ivm).
Using ivm you can install and manage multiple versions of Inko; whether you are
using Linux, macOS, or Windows. ivm is written in Rust and doesn't require
additional system dependencies.

## Installing

ivm itself only requires Rust 1.62 or newer, but to build Inko itself you'll
need to also meet the requirements listed in the [installation
guide](installation.md).

ivm is distributed using [crates.io](https://crates.io/), and is installed as
follows:

```bash
cargo install ivm
```

This will install the `ivm` executable in `$HOME/.cargo/bin`, where `$HOME` is
your home directory (`%USERPROFILE%` on Windows). You need to add this to your
shell's PATH if not done already. You also need to add the directory containing
Inko executables to your path:

=== "Bash"
    ```bash
    export PATH="$HOME/.cargo/bin:$HOME/.local/share/ivm/bin:$PATH"
    ```
=== "Fish"
    ```bash
    set -x PATH $HOME/.cargo/bin $HOME/.local/share/ivm/bin $PATH
    ```
=== "cmd.exe"
    ```dosbatch
    setx PATH "%USERPROFILE%\.cargo\bin;%LocalAppData%\ivm\bin;%PATH%"
    ```

!!! tip
    When using Windows, you need to restart your terminal after running the
    `setx` command, as it doesn't affect your current terminal.

## Updating

To update ivm, run the following:

```bash
cargo install ivm --force
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

To uninstall a version:

```bash
ivm uninstall 0.10.0    # This will uninstall version 0.10.0
ivm uninstall latest    # This will uninstall the latest _installed_ version
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
ivm run 0.10.0 inko --version    # This will run `inko --version` using Inko 0.10.0
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

For this to work the `bin` directory must be in your path, as covered in the
installation instructions. If you aren't sure where that directory is located,
run the following:

```bash
ivm show bin
```

This will print the path to the `bin` directory, which you can then add to your
PATH variable.
