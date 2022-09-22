# Error handling

Let's take a look at one of the defining features of Inko: its approach to error
handling, inspired by the article ["The Error
Model"](http://joeduffyblog.com/2016/02/07/the-error-model/). To explain this
we'll transform our "Hello, World!" program from the ["Hello,
World!"](hello-world.md) guide into a program that writes the message to a file,
reads it back, then writes it to STDOUT.

## "Hello, World!" using files

We'll start with the following code:

```inko
import 'std/fs/file' (ReadWriteFile)

class async Main {
  fn async main {

  }
}
```

Instead of importing `STDOUT` we import `ReadWriteFile`. This is a type used for
both reading and writing from and to a file. The `std/fs/file` module also
provides a type for just reading files (`ReadOnlyFile`), and a type for just
writing files (`WriteOnlyFile`). In our case we need both, hence the use of
`ReadWriteFile`.

Next we'll need to create our file:

```inko
import 'std/fs/file' (ReadWriteFile)

class async Main {
  fn async main {
    let file = try! ReadWriteFile.new('hello.txt')
  }
}
```

`ReadWriteFile.new('hello.txt')` creates a new instance of the `ReadWriteFile`
type and tells it to try and open the file `hello.txt`, creating it if it
doesn't exist.

Creating a file may fail, such as when you don't have permissions to do so. As
such the `new` method is annotated to signal that it may throw an error. The
signature of the method is as follows:

```inko
fn pub static new(path: IntoPath) !! Error -> Self {
  # ...
}
```

For now we can ignore everything except the following:

```
!! Error
```

Within a method signature, the syntax `!! TypeName` indicates the method may
throw a value of type `TypeName`. In our case the type is called `Error`. A
method may only specify a single type, simplifying the error handling process.
_If_ a method specifies a throw type, we _must_ handle the error when calling
the method, and not doing so results in a compile-time error. If a method
specifies a throw type it _must_ at some point throw a value of said type using
the `throw` or `try` keyword. Again it's a compile-time error to not do so.

These rules mean that a method can never lie about throwing or not, and every
error is guaranteed to be handled in some way. Due to the syntax used for error
handling it's also clear that a call may throw, meaning you don't have to look
at a method's definition just to figure that out.

Error handling is done using the `try` and `try!` keywords. `try` defaults to
throwing the error again:

```inko
let file = try ReadWriteFile.new('hello.txt')
```

This is subject to the same requirements for error handling as listed above.
This means you can't just use `try expression` in a method without annotating
the method accordingly.

Explicitly handling the error is done using the syntax `try EXPR else ELSE`. For
example, if we just want to return from the surrounding method we can do so as
follows:

```inko
let file = try ReadWriteFile.new('hello.txt') else return
```

You can also use curly braces for both the `try` and `else` bodies:

```inko
let file = try {
  ReadWriteFile.new('hello.txt')
} else {
  return
}
```

If we want to do something with the error, we can specify a variable to bind it
to:

```inko
let file = try ReadWriteFile.new('hello.txt') else (error) {
  return
}
```

If there's no sensible way of handling the error at runtime, we can decide to
abort the program with an error message. This is known as a panic, and we can do
this by using `try!` (that's `try` but with an exclamation mark at the end).

For this guide we just want to abort if we can't create the file, hence the use
of the `try!` keyword.

Moving on, let's write the message to the file:

```inko
import 'std/fs/file' (ReadWriteFile)

class async Main {
  fn async main {
    let file = try! ReadWriteFile.new('hello.txt')

    try! file.write_string('Hello, World!')
  }
}
```

Again the operation can fail, such as when the file is removed after creating
it, and so again we must handle any errors. And again for the sake of
simplicity, we'll just abort in the event we encounter an error.

If you now save the above code and run it, you'll end up with a file called
`hello.txt` in the current working directory, containing the text "Hello,
World!".

Let's combine this with writing the message back to STDOUT. For this we'll need
to import STDOUT again:

```inko
import 'std/fs/file' (ReadWriteFile)
import 'std/stdio' (STDOUT)

class async Main {
  fn async main {
    let file = try! ReadWriteFile.new('hello.txt')

    try! file.write_string('Hello, World!')
  }
}
```

Now we'll need to read the contents back from the file. First we must rewind it,
as reading continues where the last write (or read) ended, then we must read the
contents into a `ByteArray`:

```inko
import 'std/fs/file' (ReadWriteFile)
import 'std/stdio' (STDOUT)

class async Main {
  fn async main {
    let file = try! ReadWriteFile.new('hello.txt')

    try! file.write_string('Hello, World!')
    try! file.seek(0)

    let bytes = ByteArray.new

    try! file.read_all(bytes)
  }
}
```

Here we rewind to the start using `try! file.seek(0)`, aborting if we encounter
an error. After that we read the entire file into a `ByteArray`. As the name
suggests, `ByteArray` is a type that stores bytes. Since files can contain
virtually anything, reads operate on byte arrays instead of using strings.

To write the bytes back to STDOUT, we can use the `write_bytes` method:

```inko
import 'std/fs/file' (ReadWriteFile)
import 'std/stdio' (STDOUT)

class async Main {
  fn async main {
    let file = try! ReadWriteFile.new('hello.txt')

    try! file.write_string('Hello, World!')
    try! file.seek(0)

    let bytes = ByteArray.new

    try! file.read_all(bytes)

    STDOUT.new.write_bytes(bytes)
  }
}
```

At this point you may have noticed we don't perform any error handling when
writing to STDOUT, and you're correct: the `STDOUT` and `STDERR` types are
implemented such that they ignore any errors when writing. This is done as there
are few (if any) cases where you _don't_ want to just ignore the error and move
on.

We now have a little program that writes "Hello, World!" to a file, reads it
back, then writes it to STDOUT. But what's missing is removing the file once
we're done. And so for our next trick we'll make `hello.txt` disappear:

```inko
import 'std/fs/file' (ReadWriteFile, remove)
import 'std/stdio' (STDOUT)

class async Main {
  fn async main {
    let file = try! ReadWriteFile.new('hello.txt')

    try! file.write_string('Hello, World!')
    try! file.seek(0)

    let bytes = ByteArray.new

    try! file.read_all(bytes)

    STDOUT.new.write_bytes(bytes)

    try remove(file.path) else nil
  }
}
```

Removing files is done using the method `remove` from the `std/fs/file` module,
which we now import along with the `ReadWriteFile` type. We then remove the file
as follows:

```inko
try remove(file.path) else nil
```

This tries to remove the file, and does nothing in the event of an error. In
this case that's totally fine, as not being able to remove the file isn't a
problem.

Let's say that instead of aborting with a panic, we want to write a custom
message to STDERR and quit the program. We'd end up with something like this:

```inko
import 'std/fs/file' (ReadWriteFile, remove)
import 'std/stdio' (STDERR, STDOUT)

class async Main {
  fn async main {
    let stderr = STDERR.new
    let file = try ReadWriteFile.new('hello.txt') else {
      stderr.print('Failed to open hello.txt')
      return
    }

    try file.write_string('Hello, World!') else {
      stderr.print('Failed to write to hello.txt')
      return
    }

    try file.seek(0) else {
      stderr.print('Failed to seek to the start of hello.txt')
      return
    }

    let bytes = ByteArray.new

    try file.read_all(bytes) else {
      stderr.print('Failed to read from hello.txt')
      return
    }

    STDOUT.new.write_bytes(bytes)

    try remove(file.path) else nil
  }
}
```

If we also want to display the original IO error message, we'd end up with
something like this instead:

```inko
import 'std/fs/file' (ReadWriteFile, remove)
import 'std/stdio' (STDERR, STDOUT)

class async Main {
  fn async main {
    let stderr = STDERR.new
    let file = try ReadWriteFile.new('hello.txt') else (error) {
      stderr.print("Failed to open hello.txt: {error}")
      return
    }

    try file.write_string('Hello, World!') else (error) {
      stderr.print("Failed to write to hello.txt: {error}")
      return
    }

    try file.seek(0) else (error) {
      stderr.print("Failed to seek to the start of hello.txt: {error}")
      return
    }

    let bytes = ByteArray.new

    try file.read_all(bytes) else (error) {
      stderr.print("Failed to read from hello.txt: {error}")
      return
    }

    STDOUT.new.write_bytes(bytes)

    try remove(file.path) else nil
  }
}
```

## The cost of error handling

Error handling in Inko is cheap: the cost of a `throw` is the same as a
`return`, while a `try` consists of checking a flag and a branch. Inko doesn't
automatically attach a stack trace to every error, meaning the cost of creating
an error value is the same as creating any other value. Inko also doesn't
implicitly unwind the stack when encountering an error.

## Errors that abort execution

Inko has two types of errors: those than can be handled at runtime using `try`
or `try!`, and critical errors that abort the program. Such an error is called a
"panic", and is used for errors that shouldn't be handled by the developer.

An example of a panic is when you divide by zero, or when accessing an out of
bounds index in an array. Both cases are the result of incorrect code, and as
such all we can do is abort.

As a rule of thumb, panics should only be used when they can be triggered as the
result of incorrect code, or if there's nothing you can do other than to abort
(e.g. when your program requires a file to exist, but the file is missing).
