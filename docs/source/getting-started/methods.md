---
{
  "title": "Methods and closures"
}
---

In previous tutorials we've seen expressions such as `STDOUT.new` and
`out.print(...)`. In such expressions, `new` and `print` are method calls. But
what are methods? Well, they are functions that are bound to some object. In
case of `print`, the method is bound to an instance of `STDOUT`. This means you
first have to create an instance of `STDOUT` before you can call `print`.

Methods are defined using the `fn` keyword. An example of this which we've seen
so far is the `main` method defined like so:

```inko
class async Main {
  fn async main {

  }
}
```

We can also define methods outside of classes like so:

```inko
fn example {

}

class async Main {
  fn async main {
    example
  }
}
```

Here we define the method `example`, then call it in the `main` method. The
`example` method known as a "module method" because it's defined at the
top-level scope, which is a module (more on this later).

## Methods and classes

Within a class, we can define two types of methods: static methods, and instance
methods. Instance methods are defined as follows:

```inko
class Person {
  fn name {

  }
}
```

Meanwhile, static methods are defined using `fn static` as follows:

```inko
class Person {
  fn static new {

  }
}
```

The difference is that static methods don't require an instance of the class
they are defined in, while instance methods do. This means that to call `new` in
the above example, you'd write `Person.new`, while calling `name` would require
you to create an instance of the class, then use `person.name` where `person` is
a variable storing the instance of the class.

When a class is defined using `class async`, you can also define methods using
`fn async`. These methods are the messages you can send to a process. It's a
compile-time error to define an `fn async` method on a regular class.

## Arguments

Methods can specify one or more arguments they require:

```inko
fn person(name: String, age: Int) {

}
```

This method requires two arguments: `name` which is typed as `String`, and `age`
typed as `Int`. Inko is statically typed and requires explicit types when
defining method arguments and return types.

Methods with arguments are culled using positional arguments, named arguments,
or a mix of both:

```inko
person('Alice', 42)
person('Alice', age: 42)
person(name: 'Alice', age: 42)
```

When mixing both positional and named arguments, the positional arguments must
come first. This means the following is invalid:

```inko
person(name: 'Alice', 42)
```

Named arguments are useful when the purpose/meaning of an argument is unclear.
Take this call for example:

```inko
person('Alice', 42)
```

While we might be able to derive that "Alice" is the name of the person, it's
not clear what 42 refers to here. Using named arguments makes this more clear:

```inko
person('Alice', age: 42)
```

## Return types and values

If a method doesn't specify a return type, the compiler infers it as `Nil`. In
this case, the value returned is ignored. A custom return type is specified as
follows:

```inko
fn person(name: String, age: Int) -> String {

}
```

Here `-> String` tells the compiler this method returns a value of type
`String`.

A value is returned using either the `return` keyword, or by making it the last
expression in a method or scope (known as an "implicit return"):

```inko
fn person(name: String, age: Int) -> String {
  name
}
```

Here `name` is the last expression, so it's the return value. This is what it
looks like when using explicit returns:

```inko
fn person(name: String, age: Int) -> String {
  return name
}
```

Explicit returns are meant for returning early, for everything else you should
use implicit returns. For example:

```inko
fn person(name: String, age: Int) -> String {
  if name == 'Bob' {
    return 'Go away!'
  }

  name
}
```

When a method has an explicit return type, the value returned must be compatible
with that type. In case of our `person` method this means we must return a
`String`, and returning something else produces a compile-time error:

```inko
fn person(name: String, age: Int) -> String {
  age
}

class async Main {
  fn async main {

  }
}
```

If we try to run this program, we're presented with the following compile-time
error:

```
test.inko:2:3 error(invalid-type): expected a value of type 'String', found 'Int'
```

## Mutability

Trait and class instance methods are immutable by default, preventing them from
mutating the data stored in their receivers. For example:

```inko
class Person {
  let @name: String

  fn change_name(name: String) {
    @name = name
  }
}
```

This definition of `change_name` is invalid, as field assignments are mutations,
and `change_name` is immutable. To allow mutations, use the `mut` keyword like
so:

```inko
class Person {
  let @name: String

  fn mut change_name(name: String) {
    @name = name
  }
}
```

Module and static methods don't support the `mut` keyword. Closures don't need
it, as the ability to mutate captured variables depends on their reference types
(e.g. captured `ref` values can't be mutated).

## Taking ownership

Regular instance methods can take ownership of their receivers by defining them
using `fn move`:

```inko
class Person {
  let @name: String

  fn move into_name -> String {
    @name
  }
}
```

Methods defined using `fn move` are only available to owned and unique
references, not (im)mutable borrows. The `move` keyword isn't available to
`fn async` methods.

## Closures

Inko supports
[closures](https://en.wikipedia.org/wiki/Closure_\(computer_programming\)):
anonymous functions that (optionally) capture data, and can be moved around as
values. Closures are defined using the `fn` keyword while leaving out a name:

```inko
import std.stdio.STDOUT

class async Main {
  fn async main {
    let out = STDOUT.new

    fn { out.print('Hello!') }.call
  }
}
```

Running this program results in "Hello!" being written to the terminal. Like
regular methods, closures can also define arguments. Unlike regular methods, the
argument types and the return type are inferred:

```inko
import std.stdio.STDOUT

class async Main {
  fn async main {
    let out = STDOUT.new

    fn (message) { out.print(message) }.call('Hello!')
  }
}
```

The compiler might not always be able to infer the types though, in which case
explicit type signatures are necessary:

```inko
import std.stdio.STDOUT

class async Main {
  fn async main {
    let out = STDOUT.new

    fn (message: String) -> Int {
      out.print(message)
      42
    }.call('Hello!')
  }
}
```
