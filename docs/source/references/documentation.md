---
{
  "title": "Generating documentation"
}
---

Generating documentation from source code is done in two steps: generating a
list of JSON files that contain documentation about symbols (e.g. classes and
methods), and converting these JSON files into a desired format (e.g. a static
website).

## Writing documentation

Documentation is written using
[Markdown](https://en.wikipedia.org/wiki/Markdown), specifically using the
[inko-markdown](https://github.com/yorickpeterse/inko-markdown) dialect.

Symbols (methods, classes, etc) are documented by placing one or more comments
before them, without empty lines between the comments or between the last
comment and the start of the symbol:

```inko
# This is the documentation for the constant.
# It happens to occupy two lines.
let NUMBER = 42
```

Modules are documented by placing comments at the start of the module:

```inko
# This is the documentation for the module.
import std.string (StringBuffer)

fn example {}
```

If the module documentation is followed by a symbol (e.g. a class), ensure
there's an empty line after the comment, otherwise it's treated as the
documentation for the symbol:

```inko
# This documents the _module_ and not the class.

class Example {}
```

The following can be documented:

- Modules
- Constants
- Module methods
- Classes
- Traits
- Methods defined on a class
- Methods defined in an `impl` block
- Methods defined in a trait

## Generating the JSON files

Generating the JSON files is done by running `inko doc` in a project. The
resulting files are stored in `./build/docs`. For each module, a corresponding
JSON file is generated. For example, the documentation for `std.int` is stored
in `std_int.json`.

The `inko doc` command also generates a `$meta.json` file that contains some
additional information about the project, such as the contents of its README (if
one is present).

::: warn
Until Inko reaches version 1.0.0, the JSON structure of these files is
unspecified and may change between releases.
:::

## Converting the JSON files

The JSON files themselves are not useful for users, so you'll need a tool to
convert them to something useful.

### idoc

[idoc](https://github.com/inko-lang/idoc) is a tool that converts these JSON
files to a static website. Using idoc, you don't need to run `inko doc`
yourself, instead you just run `idoc` in your project directory and the
resulting static website is found at `./build/idoc/public`.

idoc is installed as follows:

```bash
git clone https://github.com/inko-lang/idoc.git
cd idoc
make install PREFIX=~/.local
```

This builds idoc and installs it into `~/.local`, such that the executable is
found at `~/.local/bin/idoc`.

To document your project, run `idoc` inside your project directory. If
`~/.local/bin` isn't in your `PATH` variable, run `~/.local/bin/idoc` instead.

idoc also provides a container that can be used with Docker and Podman:

```bash
# Using Docker:
docker pull ghcr.io/inko-lang/idoc:latest
docker run --rm --volume $PWD:$PWD:z --workdir $PWD idoc:latest

# Using Podman:
podman pull ghcr.io/inko-lang/idoc:latest
podman run --rm --volume $PWD:$PWD:z --workdir $PWD idoc:latest
```
