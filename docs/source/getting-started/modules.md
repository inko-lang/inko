---
{
  "title": "Modules and imports"
}
---

Inko programs are divided into many modules. A module is an Inko source file
defining methods, types, traits, constants, and more.

Modules and their symbols can be imported into other modules using the `import`
keyword. For example, in the tutorials covered so far we've seen instances of
this such as:

```inko
import std.stdio (Stdout)

type async Main {
  fn async main {
    Stdout.new.print('hello')
  }
}
```

Here we import the `Stdout` type from the module [](std.stdio). We can also
import modules as a whole:

```inko
import std.stdio

type async Main {
  fn async main {
    stdio.Stdout.new.print('hello')
  }
}
```

Importing multiple symbols at once is also possible:

```inko
import std.stdio (Stderr, Stdout)

type async Main {
  fn async main {
    Stdout.new.print('hello')
    Stderr.new.print('world')
  }
}
```

When importing different symbols with the same name, you can prevent name
conflicts by using a custom alias:

```inko
import std.stdio (Stderr as ERR, Stdout as OUT)

type async Main {
  fn async main {
    OUT.new.print('hello')
    ERR.new.print('world')
  }
}
```

## Import paths

When importing modules and symbols, the compiler looks in the following places
to find the module (in this order):

1. Your project's `src/` directory (refer to [](../references/structure) for
   more details)
1. The source directories of any dependencies of your project, as specified in
   the `inko.pkg` package manifest
1. Additional source directories specified using the `-i` / `--include` option,
   including the standard library (which is added by default)

If a module or symbol isn't found, a compile-time error is produced.
