---
{
  "title": "Pattern matching"
}
---

Pattern matching is a way to apply a pattern to a value, and destructure it in a
certain way if the pattern matches.

To start things off, create a file called `match.inko` with the following
contents:

```inko
import std.stdio (Stdout)

class async Main {
  fn async main {
    let out = Stdout.new
    let val = Option.Some((42, 'hello'))

    match val {
      case Some((42, str)) -> out.print(str)
      case _ -> out.print('oh no!')
    }
  }
}
```

Now run it using `inko run match.inko`, and the output should be as follows:

```
hello
```

In Inko we use the `match` keyword to perform pattern matching, and the `case`
keyword to specify the different cases to consider. The expression to the right
(e.g. `Some((42, str))`) is the pattern to match against. In this case `str` is
a "binding pattern", which matches against anything and assigns the matched
value to the `str` variable, which we then print to STDOUT.

::: info
`match` expressions are compiled to [decision
trees](https://en.wikipedia.org/wiki/Decision_tree). Inko's implementation is
based on [this
article](https://julesjacobs.com/notes/patternmatching/patternmatching.pdf). If
you're interested in implementing pattern matching yourself, we suggest taking a
look at [this
project](https://github.com/yorickpeterse/pattern-matching-in-rust).
:::

## Patterns

Patterns can be simple such as `42` or `str`, or more complex such as
`Some((42, str))` or `{ @name = name, @age = 20 or 30 }`. Below is a list of the
various types of supported patterns.

### Literals and constants

Pattern matching against literals is supported for values of type `Int`, `Bool`,
and `String`. We can also use constants to specify the pattern:

```inko
match 3 {
  case 0 -> 'foo'
  case 1 -> 'bar'
  case _ -> 'baz'
}

match 3 {
  case ZERO -> 'foo'
  case ONE -> 'bar'
  case _ -> 'baz'
}

match true {
  case true -> 'foo'
  case false -> 'bar'
}

match 'world' {
  case 'hello' -> 'foo'
  case 'world' -> 'bar'
  case _ -> 'baz'
}
```

When matching against values of these types, Inko treats the values as having an
infinite number of possibilities, thus requiring the use of a wildcard pattern
(`_`) to make the match exhaustive. Booleans are an exception to this, as they
only have two possible values (`true` and `false`).

### Enum patterns

If the value matched against is an enum class, we can match against its
constructors:

```inko
match Option.Some(42) {
  case Some(42) -> 'yay'
  case None -> 'nay'
}
```

In this case the compiler knows about all possible constructors, and the use of
wildcard patterns isn't needed when all constructors are covered explicitly.

When specifying the constructor pattern only its name is needed, the
name of the type it belongs to isn't needed. This means `case Option.Some(42)`
is invalid.

### Class patterns

Pattern matching can also be performed against regular classes using class
literal patterns:

```inko
class Person {
  let @name: String
  let @age: Int
}

let person = Person(name: 'Alice', age: 42)

match person {
  case { @name = name, @age = 42 } -> name
  case { @name = name, @age = age } -> name
}
```

Any fields left out are treated as wildcard patterns, meaning the following two
match expressions are the same:

```inko
match person {
  case { @name = name } -> name
}

match person {
  case { @name = name, @age = _ } -> name
}
```

Class literal patterns are only available for regular classes.

### Tuple patterns

Pattern matching against tuples is also supported:

```inko
match (10, 'testing') {
  case (num, 'testing') -> num * 2
  case (_, _) -> 0
}
```

### OR patterns

Multiple patterns can be specified at once using the `or` keyword:

```inko
match number {
  case 10 or 20 or 30 -> 'yay'
  case _ -> 'nay'
}
```

This is the same as the following match:

```inko
match number {
  case 10 -> 'yay'
  case 20 -> 'yay'
  case 30 -> 'yay'
  case _ -> 'nay'
}
```

### Bindings

Patterns can bind values to variables:

```inko
match number {
  case 10 -> 20
  case num -> num * 2
}
```

Bindings are made mutable (allowing them to be assigned new values) using `mut`:

```inko
match number {
  case 10 -> 20
  case mut num -> {
    num = 40
    num
  }
}
```

### Nested patterns

Nested patterns are also supported:

```inko
match Option.Some((10, 'testing')) {
  case Some((10, 'testing')) -> 'foo'
  case Some((num, _)) -> 'bar'
  case None -> 'baz'
}
```

## Guards

Guards allow you to set an extra condition for a pattern to be considered a
match:

```inko
import std.stdio (Stdout)

class async Main {
  fn async main {
    let out = Stdout.new
    let val = Option.Some((42, 'hello'))

    match val {
      case Some((num, _)) if num >= 20 -> out.print('A')
      case Some((num, _)) -> out.print('B')
      case _ -> out.print('oh no!')
    }
  }
}
```

If you run this program, the letter "A" is written to the terminal.

In this example, `if num >= 20` is the guard that must be met before the code
`out.print('A')` is executed. This is useful if the condition is too complex to
express as a pattern.

## Typing rules

For a given `match` expression, all pattern bodies must return a value of which
the type is compatible with the type of the first pattern's body:

```inko
match 42 {
  case 42 -> 'foo'
  case 50 -> 'bar'
  case _ -> 'baz'
}
```

Here the first pattern's body returns `'foo'`, a value of type `String`, and
thus all other pattern bodies must return value compatible with this type.

If pattern bodies return different types but you don't care about them, you can
use `nil` like so:

```inko
match 42 {
  case 42 -> {
    foo
    nil
  }
  case 50 -> {
    bar
    nil
  }
  case _ -> {
    baz
    nil
  }
}
```

## Ownership and moves

When pattern matching against an owned value, the value is moved into the match
expression. When matching the value's components, the input value is
destructured into those components:

```inko
let input = Option.Some((10, 'testing'))

match input {
  case Some((num, string)) -> num
  case _ -> 0
}
```

When the `Some((num, string))` pattern matches, the value `10` is moved into
`num`, and the value `testing` is moved into `string`. The variable `input` is
no longer available. When `num` or `string` is dropped, so is the value it
points to.

If the value matched against is a borrow, the match is performed against the
borrow. In this case any (sub) values bound are exposed as borrows as well:

```inko
let input = Option.Some((10, 'testing'))

match ref input {
  # Here `string` is of type `ref String`, not `String`, because the input is a
  # ref and not an owned value.
  case Some((num, string)) -> string
  case _ -> 0
}
```

## Drop semantics

When pattern matching, any bindings introduced as part of a pattern are dropped
at the end of the pattern's body (unless they are moved before then). When
matching against an owned value and the value is destructured, Inko performs a
"partial drop" of the outer value _before_ entering the pattern body. A partial
drop doesn't invoke the type's destructor, and doesn't drop any fields (as those
are moved into bindings or wildcard patterns as part of the match):

```inko
match Option.Some(42) {
  case Some(value) -> { # <- The Option is dropped here, but not the value
                        #    wrapped by the Some.
    value
  }
  case None -> 0
}
```

For borrows, we just drop the borrow before entering the pattern body:

```inko
match values.opt(4) {
  case Some(42) -> {
    # This is valid because the `ref Option[Int]` returned by `values.opt(4)` is
    # dropped before we enter this body. If we didn't, the line below would
    # panic, because we'd try to drop the old value of `values.opt(4)` while a
    # borrow to it still exists.
    values.set(4, Option.Some(0))
  }
  case _ -> nil
}
```

::: warn
If a pattern contains any bindings (e.g. `Some(a)` in the above case), those
bindings are dropped _at the end_ of the body. This means that if you match
against a value, bind a sub value to a variable, then drop the value (e.g. by
assigning it a new value), you'll run into a drop panic.
:::

## Exhaustiveness

When performing pattern matching, the match must be exhaustive, meaning all
possible cases must be covered:

```inko
import std.stdio (Stdout)

class async Main {
  fn async main {
    let out = Stdout.new
    let val = Option.Some((42, 'hello'))

    match val {
      case Some((42, str)) -> out.print(str)
    }
  }
}
```

If you try to run this program, you'll be presented with the following
compile-time error:

```
match.inko:8:5 error(invalid-match): not all possible cases are covered, the following patterns are missing: 'None', 'Some((_, _))'
```

In the first example we took care of making the match exhaustive using the `_`
pattern, known as a "wildcard pattern", which matches everything. We can make
the match exhaustive in a variety of ways, such as the following:

```inko
import std.stdio (Stdout)

class async Main {
  fn async main {
    let out = Stdout.new
    let val = Option.Some((42, 'hello'))

    match val {
      case Some((42, str)) -> out.print(str)
      case Some((_, str)) -> out.print(str)
      case None -> out.print('none!')
    }
  }
}
```

## Redundant patterns

Apart from requiring the match to be exhaustive, the compiler also notifies you
of redundant patterns:

```inko
import std.stdio (Stdout)

class async Main {
  fn async main {
    let out = Stdout.new
    let val = Option.Some((42, 'hello'))

    match val {
      case Some((42, str)) -> out.print(str)
      case Some((42, str)) -> out.print(str)
      case _ -> out.print('oh no!')
    }
  }
}
```

If you run this, you'll be presented with the following warning:

```
test.inko:10:7 warning(unreachable): this code is unreachable
```

The second `case` is unreachable because it's the same as the first `case`. Of
course the compiler is also able to detect more complicated redundant patterns:

```inko
import std.stdio (Stdout)

class async Main {
  fn async main {
    let out = Stdout.new
    let val = Option.Some((42, 'hello'))

    match val {
      case Some((a, _)) -> out.print('A')
      case Some((42, str)) -> out.print(str)
      case _ -> out.print('oh no!')
    }
  }
}
```

Here the patterns aren't quite the same, but the compiler is still able to
detect that the second `case` is redundant.

## Limitations

- Range patterns aren't supported, instead you can use pattern guards.
- Types defining custom destructors can't be matched against.
- `async` classes can't be matched against.
- Matching against `Float` isn't supported, as you'll likely run into
  precision/rounding errors.
- The value matched against can't be a trait.
