---
{
  "title": "Hello, world!"
}
---

For our first program, we'll start off simple by printing "Hello, world!" to the
terminal. Create a file called `hello.inko` with the following contents:

```inko
import std.stdio (Stdout)

type async Main {
  fn async main {
    Stdout.new.print('Hello, world!')
  }
}
```

To run the program, run the following command in your terminal:

```bash
inko run hello.inko
```

If all went well, the output is "Hello, world!".

## Explanation

Let's explore what the program does. We first encounter the following line:

```inko
import std.stdio (Stdout)
```

This imports the `Stdout` type, used for writing text to the terminal's standard
output stream. After the import we encounter the following:

```inko
type async Main {
  fn async main {

  }
}
```

Inko uses lightweight processes (which we'll cover separately), which are
defined using the syntax `type async NAME { ... }`. The main process is always
called "Main", and is required to define an "async" instance method called
"main".

The final line writes the message to STDOUT:

```inko
Stdout.new.print('Hello, world!')
```

`Stdout.new` creates a new instance of the `Stdout` type, and `print(...)`
prints the message to the standard output stream.
