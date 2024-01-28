---
{
  "title": "Traits"
}
---

Unlike many other languages with classes, Inko doesn't support class
inheritance. Instead, code is shared between classes using "traits". Traits are
essentially blueprints for classes, specifying what methods must be implemented
and/or providing default implementations of methods a class may wish to
override.

Let's say we want to convert different class instances into strings. We can do
so using traits:

```inko
import std.stdio.STDOUT

trait ToString {
  fn to_string -> String
}

class Cat {
  let @name: String
}

impl ToString for Cat {
  fn to_string -> String {
    @name
  }
}

class async Main {
  fn async main {
    let garfield = Cat { @name = 'Garfield' }

    STDOUT.new.print(garfield.to_string)
  }
}
```

Running this program produces the output "Garfield".

## Default methods

In this example, the `to_string` method in the `ToString` trait is a required
method. This means that any class that implements `ToString` _must_ provide an
implementation of the `to_string` method.

Default trait methods are defined as follows:

```inko
import std.stdio.STDOUT

trait ToString {
  fn to_string -> String {
    '...'
  }
}

class Cat {
  let @name: String
}

impl ToString for Cat {
  fn to_string -> String {
    @name
  }
}

class async Main {
  fn async main {
    let garfield = Cat { @name = 'Garfield' }

    STDOUT.new.print(garfield.to_string)
  }
}
```

Here the default implementation of `to_string` is to return the string `'...'`.
Our implementation of `ToString` for `Cat` still overrides it, so the output is
still "Garfield". Because the method is a default method, we can use it as-is as
follows:

```inko
import std.stdio.STDOUT

trait ToString {
  fn to_string -> String {
    '...'
  }
}

class Cat {
  let @name: String
}

impl ToString for Cat {

}

class async Main {
  fn async main {
    let garfield = Cat { @name = 'Garfield' }

    STDOUT.new.print(garfield.to_string)
  }
}
```

If we now run the program, the output is "...".

## Required traits

Traits can specify other traits that must be implemented before the trait itself
can be implemented:

```inko
trait ToString {
  fn to_string -> String
}

trait ToUpperString: ToString {
  fn to_upper_string -> String {
    to_string.to_upper
  }
}
```

Here the `ToUpperString` trait states that for a type to be able to implement
`ToUpperString`, it must also implement `ToString`.

You can also specify multiple required traits:

```inko
trait A {}
trait B {}
trait C: A + B {}
```

Here the trait `C` requires both `A` and `B` to be implemented.

## Conflicting trait methods

It's possible for different traits to define methods with the same name. If a
type tries to implement such traits, a compile-time error is produced. Inko
doesn't support renaming of trait methods as part of the implementation, so
you'll need to find a way to resolve such conflicts yourself.
