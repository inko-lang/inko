# Our first program

Let's start with the most basic Inko program one can write: "hello world". This
is a program that does one simple thing: print the text "Hello, world!" to
STDOUT.

Create a file called `hello.inko` and place it anywhere you like. We'll refer to
this file as `hello.inko` from now on. With the file in place, add the following
to it, then save it:

```inko
import std::stdio::stdout

stdout.print('Hello, world!')
```

Now open a shell and navigate to the directory containing `hello.inko`, then run
the following:

```bash
inko hello.inko
```

If all went well, this will print "Hello, world!" to STDOUT. Congratulations,
you just wrote your first Inko program!

## Diving deeper

Let's dive into our program and explain how it actually works. After all,
there's no point in writing a program if you don't understand what it does.

### Imports

We begin the program with the following line:

```inko
import std::stdio::stdout
```

This is known as an "import". In Inko, we use imports to load modules into your
own module. A module is just a file of Inko source code, and can define things
such as types, methods, and constants.

In this case, we import the module `stdout` from the `std::stdio` namespace. A
namespace is essentially a folder of one or more modules.

Coming from other programming languages, it may be a bit odd that you have to
import a module just to write data to STDOUT. This is necessary because we do
not want to clutter modules with imports that are not used. Since not every
program needs to write to STDOUT, the `stdout` module must be imported
explicitly.

In this particular case, the module we are importing is `std::stdio::stdout`.
The module is made available using the symbol `stdout`. You can import multiple
symbols (and rename them), but this will be discussed separately.

### Methods and messages

Once we have imported our module, we reach the following line:

```inko
stdout.print('Hello, world!')
```

Here `stdout` refers to the module we imported earlier on. `print` is a message
that we send to the `stdout` module, and this message takes a `String` as an
argument. In this case the argument is the `String` "Hello, world!", which will
get printed to STDOUT.

So what's the difference between a message and a method? In most cases there is
no difference, but in some cases the compiler may decide to optimise code such
that a different method is called. For this reason we use the term "messages"
and "message passing", instead of "method calls".
