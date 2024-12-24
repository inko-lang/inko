---
{
  "title": "Compile-time variables"
}
---

When building an executable, you may wish to customize certain aspects of the
program. Take the following program for example:

```inko
let PATH = '/usr/share/example'

fn load_assets_from(path: String) {
  # ...
}

type async Main {
  fn async main {
    load_assets_from(PATH)
  }
}
```

This program assumes the assets are located in `/usr/share/example`. But what if
we instead want to store them in `/usr/local/share/example`? We could change the
source code, but then we'd have to maintain a fork of the project just for this
one change.

## Changing constants at compile-time

Inko's compiler provides a solution to this problem: we can override the value
at compile-time, without changing the source code. For this to work, the
constant must meet the following requirements:

1. It must be a public constant
1. The value must be of type `String`, `Int` or `Bool`

In the above case we just need to make the constant public like so:

```inko
let pub PATH = '/usr/share/example'

fn load_assets_from(path: String) {
  # ...
}

type async Main {
  fn async main {
    load_assets_from(PATH)
  }
}
```

To specify a custom value, you use the `-d`/`--define` option when running
`inko build`. This option takes a value in the following format:

```
module.name.CONSTANT=VALUE
```

Here `module.name` is the fully qualified module name, such as `std.string` or
`std.net.socket`, `CONSTANT` is the name of the constant and `VALUE` is the
value to assign to the constant. To set `PATH` to `/usr/local/share/example`,
we'd build the program as follows:

```bash
inko build --define main.PATH=/usr/local/share/example
```

If multiple constants need to have their values adjusted, specify the
`-d`/`--define` option multiple times:

```bash
inko build --define main.PATH=/usr/local/share/example --define foo.bar.EXAMPLE=42
```

## Type requirements and conversions

The value assigned to the constant is interpreted according to the type of its
original value.

If the constant's default value is a `String`, the provided value is interpreted
as an UTF-8 string. If the default value is an `Int`, the value assigned by the
`--define` option must be a decimal number, other number formats such as
hexadecimal numbers aren't supported. For `Bool` constants the only two valid
values are `true` and `false`:

```
inko build --define foo.bar.BOOLEAN=true
```

If the value is invalid for the constant, or the constant's value can't be
overwritten (e.g. because it's a private constant), a compile-time error is
produced and the compilation process is stopped.
