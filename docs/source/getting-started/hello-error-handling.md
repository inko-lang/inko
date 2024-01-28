---
{
  "title": "Hello, error handling!"
}
---

In the previous tutorial we looked at printing a simple message to the terminal.
In this tutorial we'll expand on that example by adding basic error handling.

The need for error handling may seem redundant. Surely printing to the terminal
can't fail? Well, it can! To showcase this, we'll start with our `hello.inko`
from the previous tutorial and change it to the following:

```inko
import std.stdio.STDOUT

class async Main {
  fn async main {
    STDOUT.new.print('Hello, world!').unwrap
  }
}
```

The change made here compared to the previous tutorial is the addition of
`.unwrap` at the end of the `print()` line. We'll look into what this does
later. Now run the program as follows:

```bash
inko run hello.inko | true
```

The output of this program should be something similar to the following:

```
Stack trace (the most recent call comes last):
  [...]/hello.inko:5 in main.Main.main
  [...]/std/src/std/result.inko:119 in std.result.Result.unwrap
  [...]/std/src/std/process.inko:15 in std.process.panic
Process 'Main' (0x5637caa83150) panicked: Result.unwrap can't unwrap an Error
```

What happened here is that the `print()` failed to write to the standard output
stream. The `unwrap` method call turns such an error into a "panic". A panic is
a type of error that terminates the program. Panics are the result of bugs in
your code, and as such can't be handled at runtime. In a well written program,
such errors shouldn't occur.

## Handling the error

Writing to the terminal may fail for different reasons. We're not going to cover
these cases as that's out of the scope of this tutorial. Instead, we'll explore
how to prevent such errors from terminating our program.

In this case we have two options:

1. We just ignore the error
1. We somehow log the error in an external system

The second option relies on external systems (e.g. syslog) and this is way too
much to cover, so we'll go with the first option.

To ignore the error, change `hello.inko` to the following:

```inko
import std.stdio.STDOUT

class async Main {
  fn async main {
    let _ = STDOUT.new.print('Hello, world!')
  }
}
```

Now re-run the program like so:

```bash
inko run hello.inko | true
```

If all went well, the program should run _without_ printing anything to the
terminal.

This may seem surprising at first, but is indeed the correct behaviour: the
`print()` call still fails, but instead of calling `unwrap` we assign the result
to the variable `_`. By assigning the result of `print()` to `_`, the compiler
won't emit any warnings because the result isn't used, nor will it emit any
warnings because the variable assigned to isn't used.

::: note
These warnings aren't implemented yet. Following this pattern from the start
makes your code more future-proof.
:::

If we run the program using just `inko run hello.inko`, we get the expected
"Hello, world!" output, confirming our program still works.
