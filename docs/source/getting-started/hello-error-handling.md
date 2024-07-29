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
import std.stdio (STDOUT)

class async Main {
  fn async main {
    STDOUT.new.print('Hello, world!').get
  }
}
```

The change made here compared to the previous tutorial is the addition of `.get`
at the end of the `print()` line. We'll look into what this does later. Now run
the program as follows:

```bash
inko run hello.inko | true
```

The output of this program should be something similar to the following:

```
Stack trace (the most recent call comes last):
  [...]/hello.inko:5 in main.Main.main
  [...]/std/src/std/result.inko:119 in std.result.Result.get
  [...]/std/src/std/process.inko:15 in std.process.panic
Process 'Main' (0x5637caa83150) panicked: Result.get expects an Ok(_), but an Error(_) is found
```

What happened here is that the `print()` failed to write to the standard output
stream. The `get` method call turns such an error into a "panic". A panic is a
type of error that terminates the program. Panics are the result of bugs in your
code, and as such can't be handled at runtime. In a well written program, such
errors shouldn't occur.

Writing to the terminal may fail for different reasons. We're not going to cover
these cases as that's out of the scope of this tutorial. Instead, we'll explore
how to prevent such errors from terminating our program.

In this case we have two options:

1. We ignore the error
1. We handle it somehow

## Ignoring the error

We'll start with the first option: ignoring the error. To do so, change
`hello.inko` to the following:

```inko
import std.stdio (STDOUT)

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
`print()` call still fails, but instead of calling `get` we assign the result to
the variable `_`. By assigning the result of `print()` to `_`, the compiler
won't emit any warnings because the result isn't used, nor will it emit any
warnings because the variable assigned to isn't used.

::: note
These warnings aren't implemented yet. Following this pattern from the start
makes your code more future-proof.
:::

If we run the program using just `inko run hello.inko`, we get the expected
"Hello, world!" output, confirming our program still works.

## Handling the error

The second and (almost always) better option is to handle the error in a way
other than ignoring it.

For example, imagine a hypothetical method with the signature `log(message:
String)`, which logs the message to an external logging system (e.g. syslog). We
can use this method to log the error instead of ignoring it:

```inko
import std.stdio (STDOUT)

class async Main {
  fn async main {
    match STDOUT.new.print('Hello, world!') {
      case Ok(_) -> {}
      case Error(e) -> log(e.to_string)
    }
  }
}
```

Here we don't do anything if the call to `print` succeeds, as there's nothing
special to be done. If the call fails instead, we convert the error to a
`String` and log it.
