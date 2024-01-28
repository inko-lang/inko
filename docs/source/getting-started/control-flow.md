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
import std.stdio.STDOUT

class async Main {
  fn async main {
    let out = STDOUT.new
    let num = 42

    if num == 42 { out.print('yes') } else { out.print('no') }
  }
}
```

When you run this program, the output is "yes". If you change the value of `num`
to e.g. `50`, the output is instead "no".

Inko also supports `else if` like so:

```inko
import std.stdio.STDOUT

class async Main {
  fn async main {
    let out = STDOUT.new
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
import std.stdio.STDOUT

class async Main {
  fn async main {
    let out = STDOUT.new
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

Inko has two types of loops: unconditional loops which use the `loop` keyword,
and conditional loops that use the `while` keyword.

Here we use a conditional loop to print a number 10 times:

```inko
import std.stdio.STDOUT

class async Main {
  fn async main {
    let out = STDOUT.new
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

The following program loops indefinitely, printing an ever increasing number
every 500 milliseconds:

```inko
import std.process.(sleep)
import std.stdio.STDOUT
import std.time.Duration

class async Main {
  fn async main {
    let out = STDOUT.new
    let mut num = 0

    loop {
      out.print(num.to_string)
      num += 1
      sleep(Duration.from_millis(500))
    }
  }
}
```

You can control the iteration of a loop using the `next` and `break` keywords:
`next` jumps to the start of the next loop iteration, while `break` jumps out of
the inner-most loop:

```inko
import std.stdio.STDOUT

class async Main {
  fn async main {
    let out = STDOUT.new

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
`std.result.Result` enum, then returns it:

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

`try` takes an expression of which the type is either `std.result.Result` or
`std.option.Option`, and unwraps it. If the value is a `Result.Error` or an
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
