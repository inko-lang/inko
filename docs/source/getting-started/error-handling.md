# Error handling

Let's take a look at one of the defining features of Inko: its approach to error
handling, inspired by the article ["The Error
Model"](http://joeduffyblog.com/2016/02/07/the-error-model/). To explain this,
we'll transform our "Hello, World!" program from the ["Hello,
World!"](hello-world.md) guide into a program that writes the message to a file,
reads it back, then writes it to STDOUT.

## "Hello, World!" using files

We'll start with the following code:

```inko
import std.fs.file.ReadWriteFile

class async Main {
  fn async main {

  }
}
```

Instead of importing `STDOUT` we import `ReadWriteFile`. This is a type used for
both reading and writing from and to a file. The `std.fs.file` module also
provides a type for just reading files (`ReadOnlyFile`), and a type for just
writing files (`WriteOnlyFile`). In our case we need both, hence the use of
`ReadWriteFile`.

Next we'll need to create our file:

```inko
import std.fs.file.ReadWriteFile

class async Main {
  fn async main {
    let file = ReadWriteFile.new('hello.txt').expect('failed to create the file')
  }
}
```

`ReadWriteFile.new('hello.txt')` creates a new instance of the `ReadWriteFile`
type and tells it to try and open the file `hello.txt`, creating it if it
doesn't exist.

Creating a file may fail, such as when you don't have permissions to do so. As
such, the `new` method returns a `Result[ReadWriteFile, Error]`, where `Error`
is the `std.io.Error` type:

```inko
fn pub static new(path: IntoPath) -> Result[ReadWriteFile, Error] {
  # ...
}
```

The `Result` type is a regular algebraic type, similar to `Option`. To use the
underlying value, we have to pattern match against the `Result` value.

Errors can be handled in a few different ways. The most verbose approach is to
pattern match using `match`:

```inko
match ReadWriteFile.new('hello.txt') {
  case Ok(file) -> ...
  case Error(error) -> ...
}
```

We can also use the `try` keyword to return a new `Result` containing the error
value, if an error occurred:

```inko
try ReadWriteFile.new('hello.txt')
```

`try` is only available if the surrounding method or closure's return type is a
`Result` or `Option`. `try expr` works as follows:

- If `expr` is a `Result` and the case is `Error`, return the `Error` case,
  otherwise unwrap the `Ok`.
- If `expr` is an `Option` and the case is `None`, return a `None`, otherwise
  unwrap the `Some`.

For example, this:

```inko
let result: Result[Int, String] = Result.Ok(42)

try result
```

Is the same as this:

```inko
let result: Result[Int, String] = Result.Ok(42)

match result {
  case Ok(val) -> val
  case Error(err) -> return Result.Error(err)
}
```

We can also use methods, such as `unwrap` and `expect`, to get the underlying
value and panic if an error is encountered:

```inko
ReadWriteFile.new('hello.txt').unwrap           # Panic with a default error message
ReadWriteFile.new('hello.txt').expect('oh no!') # Panic with the given message
```

In general you'll want to avoid using `unwrap` and related methods in libraries,
and only use it in executables if you're certain the error won't occur _or_
there's no better option than terminating the program in the event of an error.

For the sake of brevity we'll use `expect` in the rest of this guide.

Moving on, let's write the message to the file:

```inko
import std.fs.file.ReadWriteFile

class async Main {
  fn async main {
    let file = ReadWriteFile.new('hello.txt').expect('failed to create the file')

    file.write_string('Hello, World!').expect('failed to write to the file')
  }
}
```

Again the operation can fail, such as when the file is removed after creating
it, and so again we must handle any errors. And again, for the sake of
simplicity, we'll just abort in the event we encounter an error.

If you now save the above code and run it, you'll end up with a file called
`hello.txt` in the current working directory, containing the text "Hello,
World!".

Let's combine this with writing the message back to STDOUT. For this we'll need
to import STDOUT again:

```inko
import std.fs.file.ReadWriteFile
import std.stdio.STDOUT

class async Main {
  fn async main {
    let file = ReadWriteFile.new('hello.txt').expect('failed to create the file')

    file.write_string('Hello, World!').expect('failed to write to the file')
  }
}
```

Now we'll need to read the contents back from the file. First we must rewind it,
as reading continues where the last write (or read) ended; then we must read the
contents into a `ByteArray`:

```inko
import std.fs.file.ReadWriteFile
import std.stdio.STDOUT

class async Main {
  fn async main {
    let file = ReadWriteFile.new('hello.txt').expect('failed to create the file')

    file.write_string('Hello, World!').expect('failed to write to the file')
    file.seek(0).expect('failed to rewind the file cursor')

    let bytes = ByteArray.new

    file.read_all(bytes).expect('failed to read the file')
  }
}
```

Here we rewind to the start using `file.seek(0).expect(...)`, aborting if we
encounter an error. After that we read the entire file into a `ByteArray`. As
the name suggests, `ByteArray` is a type that stores bytes. Since files can
contain virtually anything, reads operate on byte arrays instead of using
strings.

To write the bytes back to STDOUT, we can use the `write_bytes` method:

```inko
import std.fs.file.ReadWriteFile
import std.stdio.STDOUT

class async Main {
  fn async main {
    let file = ReadWriteFile.new('hello.txt').expect('failed to create the file')

    file.write_string('Hello, World!').expect('failed to write to the file')
    file.seek(0).expect('failed to rewind the file cursor')

    let bytes = ByteArray.new

    file.read_all(bytes).expect('failed to read the file')
    STDOUT.new.write_bytes(bytes).expect('failed to write to STDOUT')
  }
}
```

We now have a little program that writes "Hello, World!" to a file, reads it
back, then writes it to STDOUT. But what's missing is removing the file once
we're done. And so for our next trick we'll make `hello.txt` disappear:

```inko
import std.fs.file.(ReadWriteFile, remove)
import std.stdio.STDOUT

class async Main {
  fn async main {
    let file = ReadWriteFile.new('hello.txt').expect('failed to create the file')

    file.write_string('Hello, World!').expect('failed to write to the file')
    file.seek(0).expect('failed to rewind the file cursor')

    let bytes = ByteArray.new

    file.read_all(bytes).expect('failed to read the file')
    STDOUT.new.write_bytes(bytes).expect('failed to write to STDOUT')

    let _ = remove(file.path)
  }
}
```

Removing files is done using the method `std.fs.file.remove`, which we now
import along with the `ReadWriteFile` type. Because failing to remove the file
isn't a big deal, we ignore the `Result` returned by it by assigning it to `_`.

!!! tip
    At the moment, the compiler doesn't enforce using a `Result` when it's
    returned. In the future it will be an error to ignore `Result` values. To
    future-proof your code, make sure to assign `Result` values that can be
    ignored to `_`.

## Producing errors

When using the `Result` type for error handling, there are two ways we can
signal an error:

1. Using a regular `return`: `return Result.Error('oh no!')`
1. Using the `throw` keyword: `throw 'oh no!'`

Using `throw x` is the same as `return Result.Error(x)`, but saves you a bit of
typing.

If a method returns an `Option`, the `throw` keyword can't be used as the `None`
case of `Option` doesn't wrap a value. In this case you have to use a regular
`return Option.None`.

## The cost of error handling

Error handling involves pattern matching, which does incur a runtime cost,
though the cost may not matter much on modern hardware with good branch
predictors. `Result` types are also heap allocated at the moment, but we hope to
optimise this away in future releases.

## Errors that abort execution

Inko has two types of errors: those than can be handled at runtime using `try`
or `match`, and critical errors that abort the program. Such an error is called
a "panic", and is used for errors that shouldn't be handled by the developer.

An example of a panic is when you divide by zero, or when accessing an out of
bounds index in an array. Both cases are the result of incorrect code, and as
such all we can do is abort.

As a rule of thumb, panics should only be used when they can be triggered as the
result of incorrect code, or if there's nothing you can do other than to abort
(e.g. when your program requires a file to exist, but the file is missing).
