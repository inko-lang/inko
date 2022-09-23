# Hello, World!

Let's start with the most basic Inko program: a program that writes
"Hello, World" to STDOUT.

Create a file called `hello.inko` with the following contents:

```inko
import 'std/stdio' (STDOUT)

class async Main {
  fn async main {
    STDOUT.new.print('Hello, World!')
  }
}
```

Now run it as follows:

```bash
inko run hello.inko
```

If all went well, the output is "Hello, World!".

## Breaking it down

Let's break down what the program does. We first encounter the following line:

```inko
import 'std/stdio' (STDOUT)
```

This imports the `STDOUT` type from the `std/stdio` module, provided by the
standard library. Inko doesn't expose a print method of sorts by default, as not
every program needs to write to STDOUT or STDERR. As such, we have to explicitly
import the necessary type. If we wanted to write to STDERR, we'd instead use the
following import:

```inko
import 'std/stdio' (STDERR)
```

We can also import both:

```inko
import 'std/stdio' (STDERR, STDOUT)
```

After the import we encounter the following:

```inko
class async Main {
  fn async main {

  }
}
```

Inko uses lightweight processes (more on that later), which are defined using
the syntax `async class NAME { ... }`. The main process is always called "Main",
and is required to define an "async" instance method called "main".

The final line writes the message to STDOUT:

```inko
STDOUT.new.print('Hello, World!')
```

`STDOUT` is a regular class, and to use it we must first create an instance of
it using the `new` static method. The `print` method is then used to write the
given `String` to STDOUT.
