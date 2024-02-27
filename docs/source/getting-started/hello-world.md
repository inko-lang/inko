---
{
  "title": "Hello, world!"
}
---

For our first program, we'll start off simple by printing "Hello, world!" to the
terminal. Create a file called `hello.inko` with the following contents:

```inko
import std.stdio (STDOUT)

class async Main {
  fn async main {
    STDOUT.new.print('Hello, world!')
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
import std.stdio (STDOUT)
```

This imports the `STDOUT` type, used for writing text to the terminal's standard
output stream. After the import we encounter the following:

```inko
class async Main {
  fn async main {

  }
}
```

Inko uses lightweight processes (which we'll cover separately), which are
defined using the syntax `class async NAME { ... }`. The main process is always
called "Main", and is required to define an "async" instance method called
"main".

The final line writes the message to STDOUT:

```inko
STDOUT.new.print('Hello, world!')
```

`STDOUT.new` creates a new instance of the `STDOUT` type, and `print(...)`
prints the message to the standard output stream.
