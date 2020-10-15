# Modules

A module is a collection of source code, such as types and methods, in its
own namespace. Each Inko source file is automatically a module. There is no way
to define a module manually.

## Module names

The name of a module is derived from the path (relative to a source directory)
of the Inko source file the module belongs to. Imagine your project structure is
as follows:

```
src/
└── foo/
    └── bar.inko
```

Here `src/` is added to the list of directories the compiler will search for
Inko source files. The `foo/` directory defines a namespace (`foo`), and
contains a single module: `bar`. In Inko, the namespace separator is `::`,
meaning the full name of this module is `foo::bar`.

The methods and types in a module are always public, and there is no way to
declare these as private. If your module relies on certain types or methods that
you don't want to expose as part of the public API, moving these types and/or
methods to a separate module is the best solution. For example, the module
`std::fs::file` relies on various methods provided by the module
`std::fs::bits`.

!!! info
    We are considering adding support for private methods and constants, though
    this is not a priority for the time being. For more information, take a look
    at the issue ["Private constants and
    methods"](https://gitlab.com/inko-lang/inko/-/issues/162).

## Referring to the current module

Inside a module, `self` refers to the module itself:

```inko
def foo {}

self.foo # Same as just `foo`
```

You can also use the global `ThisModule`, which is automatically defined for
every module and refers to the module itself:

```inko
def foo {}

ThisModule.foo # Same as `self.foo`, which is the same as `foo` in this case
```

This is useful if we want to send a message to the module, but the lack of an
explicit receiver would cause a conflict:

```inko
def example -> Integer {
  10
}

object Example {
  def example -> Integer {
    # This ensures we return `10`, instead of recursing back into the current
    # method.
    ThisModule.example
  }
}
```

## Importing modules

Modules can import other modules, as well as their types; optionally binding
them using a different name. You can import a module using the `import` keyword.

Imports can only occur at the top-level of a module. So this is fine:

```inko
import foo
```

But this is not:

```inko
def example {
  import foo
}
```

Importing a module itself is done as follows:

```inko
import std::fs::file
```

The `file` module can then be accessed using the `file` global.

Importing a single constant from a module (instead of importing the module as a
whole) is done as follows:

```inko
import std::fs::file::ReadOnlyFile
```

If you want to import multiple constants, you can do so as follows:

```inko
import std::fs::file::(ReadOnlyFile, WriteOnlyFile)
```

If you want to bind a constant to a different name, you can do so as follows:

```inko
import std::fs::file::(ReadOnlyFile as File)

File         # => OK
ReadOnlyFile # => undefined
```

If you want to import both a module and some of its constants, you can do so as
follows:

```inko
import std::fs::file::(self, ReadOnlyFile)
```

You can also import a module as a whole and bind it to a different name:

```inko
import std::fs::file::(self as foo)
```

!!! info
    Importing of methods isn't supported. For more information, refer to the
    issue ["Allow importing of just methods from a
    module"](https://gitlab.com/inko-lang/inko/-/issues/158).

## Import order

Imports are always processed before executing any code in a module, in the same
order as the `import` statements. This means that this:

```inko
import std::stdio::stdout

stdout.print('hello')

import std::fs::file
```

Is executed as if it were written like this:

```inko
import std::stdio::stdout
import std::fs::file

stdout.print('hello')
```
