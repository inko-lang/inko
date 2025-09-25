---
{
  "title": "Methods and closures"
}
---

In previous tutorials we've seen expressions such as `Stdout.new` and
`out.print(...)`. In such expressions, `new` and `print` are method calls. But
what are methods? Well, they are functions that are bound to some object. In
case of `print`, the method is bound to an instance of `Stdout`. This means you
first have to create an instance of `Stdout` before you can call `print`.

Methods are defined using the `fn` keyword. An example of this which we've seen
so far is the `main` method defined like so:

```inko
type async Main {
  fn async main {

  }
}
```

We can also define methods outside of types like so:

```inko
fn example {

}

type async Main {
  fn async main {
    example
  }
}
```

Here we define the method `example`, then call it in the `main` method. The
`example` method known as a "module method" because it's defined at the
top-level scope, which is a module (more on this later).

## Methods and types

Within a type, we can define two types of methods: static methods, and instance
methods. Instance methods are defined as follows:

```inko
type Person {
  fn name {

  }
}
```

Meanwhile, static methods are defined using `fn static` as follows:

```inko
type Person {
  fn static new {

  }
}
```

The difference is that static methods don't require an instance of the type
they are defined in, while instance methods do. This means that to call `new` in
the above example, you'd write `Person.new`, while calling `name` would require
you to create an instance of the type, then use `person.name` where `person` is
a variable storing the instance of the type.

When a type is defined using `type async`, you can also define methods using
`fn async`. These methods are the messages you can send to a process. It's a
compile-time error to define an `fn async` method on a regular type.

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

type async Main {
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

Trait and type instance methods are immutable by default, preventing them from
mutating the data stored in their receivers. For example:

```inko
type Person {
  let mut @name: String

  fn change_name(name: String) {
    @name = name
  }
}
```

This definition of `change_name` is invalid, as field assignments are mutations,
and `change_name` is immutable. To allow mutations, use the `mut` keyword like
so:

```inko
type Person {
  let mut @name: String

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
type Person {
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
import std.stdio (Stdout)

type async Main {
  fn async main {
    let out = Stdout.new

    fn { out.print('Hello!') }.call
  }
}
```

Running this program results in "Hello!" being written to the terminal. Like
regular methods, closures can also define arguments. Unlike regular methods, the
argument types and the return type are inferred:

```inko
import std.stdio (Stdout)

type async Main {
  fn async main {
    let out = Stdout.new

    fn (message) { out.print(message) }.call('Hello!')
  }
}
```

The compiler might not always be able to infer the types though, in which case
explicit type signatures are necessary:

```inko
import std.stdio (Stdout)

type async Main {
  fn async main {
    let out = Stdout.new

    fn (message: String) -> Int {
      out.print(message)
      42
    }.call('Hello!')
  }
}
```

### Capturing variables

By default, closures capture borrows of variables _by value_. What this means is
that the variable can still be used outside of the closure, but assignments to
the variable inside the closure aren't available outside of it. To prevent this
from causing bugs, the compiler produces a compile-time error when you try to
assign captured variables a new value:

```inko
type async Main {
  fn async main {
    let mut a = 10

    fn { a = 20 }.call
  }
}
```

This produces:

```
test.inko:5:10 error(invalid-assign): variables captured by non-moving closures can't be assigned new values
```

To resolve this, we have to _move_ the captured variable into the closure. This
is done by using the `fn move` keyword. When using `fn move`, you _can_ assign
the captured variable a new value:

```inko
type async Main {
  fn async main {
    let mut nums = [10]

    fn move { nums = [20] }.call
  }
}
```

Because `nums` is _moved_ into the closure, you can no longer use it outside of
it:

```inko
type async Main {
  fn async main {
    let mut nums = [10]

    fn move { nums = [20] }.call

    nums # => error: 'nums' can't be used as it has been moved
  }
}
```

### Exposing captures

When a variable is captured, they are exposed by value for `copy` types such as
`Int` and `String`, but by borrow for other types. If the captured variable is
an owned value or a mutable borrow, it's exposed as a mutable borrow:

```inko
let a = 10
let b = [20]
let c = ref b

fn {
  a # => Int
  b # => mut Array[Int]
  c # => ref Array[Int]
}
```

When defining a closure using `fn move`, variables are moved into the closure
but they're still exposed as a borrow when necessary:

```inko
let a = 10
let b = [20]
let c = ref b

fn move {
  a # => Int
  b # => mut Array[Int]
  c # => ref Array[Int]
}
```

The reason for this is that while the captured variables are moved into the
closure, closures can still be called multiple times and thus the variables have
to be exposed as borrows.

### Moving closures

By default closures don't allow moving of captured variables out of the closure,
and captures are exposed to the closure body as borrows. There's a way around
that: when passing a closure literal to an argument typed as `fn move`, the
closure is inferred as a closure that can only be called once. Such closures are
called "moving closures".

::: tip
There's no dedicated syntax for creating moving closures. Closure literals
created using `fn move { ... }` are _regular_ closures that _capture_ by moving,
rather than moving captures _out_ of the closure.
:::

Unlike regular closures, moving closure expose captured variables using their
original type, similar to `fn move` methods. Moving closures _always_ capture
owned and unique values by moving them:

```inko
fn example(fun: fn move) {
  fun.call
}

let a = [10, 20]

example(fn { a }) # This is OK because the expected closure type is `fn move`
a.size            # Not OK because `a` is moved into the closure
```

This is only supported when _directly_ passing a closure literal to an `fn move`
argument. If the literal is first assigned to a variable or passed around then
it _won't_ be inferred as a moving closure:


```inko
fn example(fun: fn move) {
  fun.call
}

let a = [10, 20]
let b = fn { a }

example(b) # Not OK as `b` is not inferred as an `fn move` closure
```
