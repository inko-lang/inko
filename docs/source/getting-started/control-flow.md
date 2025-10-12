---
{
  "title": "Control flow"
}
---

Inko has the following control flow constructs: `if`, `and`, `or`, `while`,
`loop`, `try`, and `throw`.

## Conditionals

For conditionals we use `if`:

```inko
import std.stdio (Stdout)

type async Main {
  fn async main {
    let out = Stdout.new
    let num = 42

    if num == 42 { out.print('yes') } else { out.print('no') }
  }
}
```

When you run this program, the output is "yes". If you change the value of `num`
to e.g. `50`, the output is instead "no".

Inko also supports `else if` like so:

```inko
import std.stdio (Stdout)

type async Main {
  fn async main {
    let out = Stdout.new
    let num = 50

    if num == 42 {
      out.print('A')
    } else if num == 50 {
      out.print('B')
    } else {
      out.print('C')
    }
  }
}
```

The output of this program is "B".

To perform boolean AND and OR operations, you can use the `and` and `or`
keywords:

```inko
import std.stdio (Stdout)

type async Main {
  fn async main {
    let out = Stdout.new
    let num = 50

    if num == 42 or num == 50 {
      out.print('A')
    } else if num >= 10 and num <= 20 {
      out.print('B')
    } else {
      out.print('C')
    }
  }
}
```

This prints "A" if you run the program as-is, and "B" if you change `num` to
`20`.

## Loops

Inko has three types of loops: `for`, `while` and `loop`. Loop iteration is
controlled using the `break` and `next` keywords.

### for

The `for` expression is used to iterate over values provided by an iterator. For
example, to iterate over the values in an array (taking over ownership of the
array in the process):

```inko
import std.stdio (Stdout)

type async Main {
  fn async main {
    let out = Stdout.new

    for num in [10, 20, 30] {
      out.print(num.to_string)
    }
  }
}
```

`for` loops are just syntax sugar, and the above `for` loop is compiled to the
following:

```inko
let __iter = [10, 20, 30].into_iter

loop {
  match __iter.next {
    case Some(num) -> print(num)
    case _ -> break
  }
}
```

A `for` loop supports any collection/iterator that implements the `into_iter`
method, which in turn is expected to return a type that implements the
[](std.iter.Iter) trait.

`for` loops can use pattern matching to match against specific values returned
by `next`:

```inko
import std.stdio (Stdout)

type async Main {
  fn async main {
    let out = Stdout.new

    for (str, _) in [('foo', 10), ('bar', 20)] {
      out.print(str)
    }
  }
}
```

If the pattern doesn't match the value returned by `next`, iteration stops:

```inko

import std.stdio (Stdout)

type async Main {
  fn async main {
    let out = Stdout.new

    for (str, 10) in [('foo', 10), ('bar', 20)] {
      out.print(str)
    }
  }
}
```

This prints `foo` to STDOUT then iteration stops, as the pattern only matches
the first value in the array.

### while

A `while` loop is a loop that repeats itself for as long as its condition
evaluates to `true`:

```inko
import std.stdio (Stdout)

type async Main {
  fn async main {
    let out = Stdout.new
    let mut num = 0

    while num < 10 {
      out.print(num.to_string)
      num += 1
    }
  }
}
```

The output of this program is as follows:

```
0
1
2
3
4
5
6
7
8
9
```

### loop

The `loop` keyword is used to create a loop that runs indefinitely. The
following program loops indefinitely, printing an ever increasing number every
500 milliseconds:

```inko
import std.process (sleep)
import std.stdio (Stdout)
import std.time (Duration)

type async Main {
  fn async main {
    let out = Stdout.new
    let mut num = 0

    loop {
      out.print(num.to_string)
      num += 1
      sleep(Duration.from_millis(500))
    }
  }
}
```

### break and next

You can control the iteration of a loop using the `next` and `break` keywords:
`next` jumps to the start of the next loop iteration, while `break` jumps out of
the inner-most loop:

```inko
import std.stdio (Stdout)

type async Main {
  fn async main {
    let out = Stdout.new

    loop {
      out.print('hello')
      break
    }
  }
}
```

This program prints "hello", then stops the loop.

## throw

`throw` takes an expression and wraps it in the `Error` constructor of the
[](std.result.Result) enum, then returns it:

```inko
fn example -> Result[Int, String] {
  throw 'oh no!'
}
```

This is the equivalent of the following:

```inko
fn example -> Result[Int, String] {
  return Result.Error('oh no!')
}
```

The `throw` keyword is only available in methods of which the return type is a
`Result`.

## try

`try` takes an expression of which the type is either [](std.result.Result) or
[](std.option.Option), and gets it. If the value is a `Result.Error` or an
`Option.None`, the value is returned as-is.

Consider this example of using `try` with an `Option` value:

```inko
let value = Option.Some(42)

try value
```

This is the equivalent of:

```inko
let value = Option.Some(42)

match value {
  case Some(v) -> v
  case None -> return Option.None
}
```

And when using a `Result`:

```inko
let value = Result.Ok(42)

try value
```

This is the equivalent of:

```inko
let value = Result.Ok(42)

match value {
  case Ok(v) -> v
  case Error(e) -> return Result.Error(e)
}
```

## Conditional moves

If a variable is dropped conditionally, it's not available afterwards:

```inko
let a = [10]

if something {
  let b = a
}

# `a` _might_ be moved at this point, so we can't use it anymore.
```

The same applies to loops: if a variable is moved in a loop, it can't be used
outside the loop:

```inko
let a = [10]

loop {
  let b = a
}
```

Any variable defined outside of a loop but moved inside the loop _must_ be
assigned a new value before the end of the loop. This means the above code is
incorrect, and we have to fix it like so:

```inko
let mut a = [10]

loop {
  let b = a

  a = []
}
```

We can do the same for conditions:

```inko
let mut a = [10]

if condition {
  let b = a

  a = []
}

# `a` can be used here, because we guaranteed it always has a value at this
# point
```

If a value is moved in one branch of a condition, it remains available in the
other branches:

```inko
let a = [10]

# This is fine, because only one branch ever runs.
if foo {
  let b = a
} else if bar {
  let b = a
}
```
