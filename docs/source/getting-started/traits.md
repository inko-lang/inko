# Traits

We've briefly covered traits in the [Basic types](basic-types.md) chapter. In
this chapter we'll take a closer look at them.

Traits are like interfaces found in other languages, with one key difference:
you can define default method implementations in a trait. Using traits we can
compose behaviour, without the limitations and complications of inheritance.

## Defining traits

You can define a trait using the `trait` keyword, followed by its name and a
pair of curly braces:

```inko
trait ToString {

}
```

You can't create an instance of a trait. Instead, traits act like blueprints to
be implemented by classes.

## Implementing traits

A trait is implemented using the `impl` keyword, like so:

```inko
trait ToString {

}

class Number {

}

impl ToString for Number {

}
```

Inko uses nominal typing, so a trait is not implemented unless you explicitly do
so using the `impl` keyword.

A trait can only be implemented once for a class. If a method in a trait
conflicts with that of another trait, the trait can't be implemented; requiring
you to resolve the conflict somehow.

!!! note
    Renaming methods when implementing traits is not supported. We may add
    support for this in the future.

## Trait requirements

When defining a trait, you can specify one or more traits as requirements. When
a class implements a trait with one or more of such requirements, the class
must also implement those traits:

```inko
trait ToString {

}

trait Display: ToString {

}
```

Here the `Display` trait states that the `ToString` trait must also be
implemented. You can also specify multiple requirements:

```inko
trait ToString {

}

trait Format {

}

trait Display: ToString + Format {

}
```

## Required methods

Traits can define methods that a class must implement. To define a required
method, use the `def` keyword like regular methods and leave out the method
body:

```inko
trait ToString {
  def to_string -> String
}
```

Required methods can also specify arguments:

```inko
trait Printer {
  def print(value: String) -> String
}
```

Unlike regular methods, required methods can't use default values for arguments.
This means the following is invalid:

```inko
trait Printer {
  def print(value = 'foo') -> String
}
```

## Default methods

We define default methods like any other method. When a class implements a trait
with a default method, that method becomes available to the object. An object
is free to redefine the default method's implementation:

```inko
trait ToString {
  def to_string -> String {
    ''
  }
}

class Fireworks {

}

impl ToString for Fireworks {
  def to_string -> String {
    'Boom!'
  }
}
```
