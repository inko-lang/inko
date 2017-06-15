# Inko

**NOTE:** Inko is still a work in progress and in the very early stages. For
example, there's no website just yet, the compiler is in the super early stages,
etc.

Inko is a gradually typed, interpreted, concurrent, object oriented programming
language that combines the flexibility of a dynamic language with the safety of
a static language. Inko at its core is a prototype-based language, drawing heavy
inspiration from other languages such as Smalltalk, Self, Ruby, Erlang, and
Rust.

## Concurrency

Inko's concurrency model is heavily inspired by Erlang and relies on lightweight
processes managed by the virtual machine. Each process can perform a certain
amount of work before it is suspended and another process is executed. Work is
evenly balanced amongst multiple OS threads, and these threads may steal jobs
from each other. This ensures load is balanced evenly and you won't end up with
a single thread performing all the work while the rest idles.

## Object Oriented

Inko is a prototype-based object oriented programming language. Since
prototype-based languages are a bit hard to work with Inko provides a simple
class system, allowing you to organise your code similar to other languages.
This means you will very rarely have to use prototypes directly, and you don't
have to invent your own way of defining classes or similar structures.

Inko relies heavily on message passing and provides no traditional `if`, `while`
and similar statements. Instead you send messages to objects. For example, a
simple if statement looks as follows:

    x.if true: {
      ...
    },
    false: {
      ...
    }

Here `if` is a message sent to `x`, and `true:` and `false:` are keyword
arguments that each take a closure.

A while statement in turn is written as follows:

    { condition-here }.while_true {
      ...
    }

Here `while_true` is a message sent to the receiving closure, and the argument
is the closure to evaluate should the condition evaluate to true.

This allows objects to define whether they should evaluate to true or false, and
simplifies the syntax greatly. In fact, Inko only has 14 reserved keywords!

## Error Handling

For error handling Inko uses exceptions and is heavily inspired by the article
["The Error Model"][error-model] by Joe Duffy. The way you handle exceptions in
Inko is a bit different and much more strict compared to other languages.

First of all, any method that may throw an error _must_ specify this in the type
signature:

    # This will produce a compile time error because the method's signature does
    # not specify what can be thrown.
    fn ping {
      throw NetworkTimeout.new
    }

    # This however is valid.
    fn ping throw NetworkTimeout {
      throw NetworkTimeout.new
    }

Second, a method that specifies it may throw an error must actually use the
`throw` keyword. This means that the following code is invalid:

    fn ping throw NetworkTimeout {
      'nope'
    }

Third, a method can _only_ throw a single type. This means that the following is
not valid:

    fn ping throw Foo, Bar, Baz {
      ...
    }

You _can_ however use a trait in the signature. This will allow the method to
throw any type as long as those types implement the given trait. This ensures
that a caller only has to deal with a single type, removing the need for giant
try-catch blocks.

Calling a method that may throw an error requires you to prefix the call with
the `try` keyword like so:

    try ping

This makes it crystal clear to the reader that `ping` may throw an error. The
default behaviour of this keyword is to re-raise the error, which in turn is
bound by the rules above. This means that this code is invalid because the first
method does not specify the type it may throw:

    fn foo {
      ping
    }

    fn ping throw NetworkError {
      ...
    }

Custom behaviour in the event of an error can be specified using the `else`
keyword:

    let x = try ping else something_else

Here the `ping` message will be sent, and in the event of an error the
`something_else` message will be sent. The `else` keyword also takes a single
(optional) argument which will contain the value thrown:

    let x = try ping else (error) {
      ...
    }

Here the `error` argument will contain the error, and is only available to the
block that follows it.

For longer snippets of code you can also use curly braces:

    let x = try {
      lots_of_code_here
    }
    else (error) {
      ...
    }

In all cases the return types of the `try` and `else` blocks must match.

If an error bubbles up all the way to the top of a process the process will
panic, resulting in the entire program terminating.

This brings us to the final part of error handling: panics. A panic is an error
that will result in the entire program terminating. Panics are used whenever an
error occurs that can not be handled reasonably at run time. For example, zero
division errors are panics because they are the result of incorrect program
behaviour.

In Inko one should only use exceptions for errors that are expected to occur
from time to time. Examples include network timeouts, file permission errors and
input validation errors. Panics in turn should be used for everything else.

## Mutability

Unlike many other OO languages data in Inko is immutable by default, requiring
you explicitly mark it as mutable. For example, the `let` keyword can be used to
define an immutable variable while `var` can be used to define a mutable one:

    let a = 10
    var b = 10

    a = 20 # => error
    b = 20 # => this is OK

The same applies to method arguments, which are immutable by default but can be
made mutable using the `var` keyword:

    fn append_to(var array) {
      ...
    }

## Gradually Typed

Inko is gradually typed, with static typing being the default. This means you
need to explicitly opt-in for dynamic typing, providing a safer default. Using
dynamic typing is as simple as leaving out type signatures. For example, this
method uses static types:

    fn add(left: Integer, right: Integer) -> Integer {
      ...
    }

This method however uses dynamic types:

    fn add(left, right) {
      ...
    }

For variables however you need to use the special `Dynamic` type as by default
the type is inferred based on the value. This means that instead of this:

    let x = something

You will have to write:

    let x: Dynamic = something

Dynamic typing does not automatically allow the reassignment of variables, for
this you will need to use the `var` keyword:

    var x = 10
    x = 20

## Garbage Collection

Inko is a garbage collected language and uses [Immix][immix] as the algorithm.
The garbage collector is a parallel and mostly concurrent garbage collector.
There is no stop-the-world phase that will pause all threads, instead the
garbage collector will only suspend the process that is being collected,
allowing others to continue running. Garbage collection is performed in
parallel to reduce the time a process is suspended.

The garbage collector also comes with instrumentation, tracking the amount of
time spent in preparing a collection, tracing through live objects, etc. This
data is currently not yet exposed but will be in the future.

## Code Organisation

Organising logic is done by creating classes and traits, and by having classes
implement these traits where necessary. Class inheritance is not supported as
traits provide a better mechanism for code reuse. For example, instead of
creating an (abstract) base class to provide reusable functionality you would
define a trait instead.

As an example, instead of a base Object class providing a `to_string` method
that is overwritten in child classes there is a ToString trait, defined as
follows:

    trait ToString {
      fn to_string -> String
    }

Each class that wishes to provide a `to_string` method can then simply implement
the trait:

    import std::string::ToString

    class MyClass impl ToString {
      fn to_string -> String {
        ...
      }
    }

## Syntax

Inko uses curly braces for blocks, and only has 14 keywords. Most of the
language relies heavily on message passing. Variables are defined using `let`
and `var`:

    let a = 10
    var b = 20

Methods are defined using the `fn` keyword:

    fn method_name {

    }

Closures use the same syntax, except they don't include a name. If no arguments
are specified you can even leave out the `fn` keyword:

    let a = {
      ...
    }

    let b = fn(arg) {
      ...
    }

Parenthesis are used for passing arguments but are optional:

    receiver.message 10
    receiver.message(10)

Arguments can either be positional arguments, or keyword arguments:

    receiver.message 10
    receiver.message number: 10

If no parenthesis are specified then the arguments list will terminate at the
end of the line.

Every argument is also a keyword argument, so you can use both whenever you
like. It's preferred to use keyword arguments when the meaning of the arguments
may not be clear otherwise:

    # Here we can infer the meaning of the arguments quite easily.
    'hello world'.replace('hello', 'HELLO')

    # Here however things get a bit more tricky, especially if our list of
    # arguments grows.
    Person.new('Alice', 24, '5th Street')

    # In these cases using keyword arguments can make things more clear:
    Person.new(name: 'Alice', age: 24, address: '5th Street')

Classes and traits are defined using the `class` and `trait` keywords
respectively. The `impl` keyword can be used to implement a trait, and takes an
optional list of methods to rename:

    class Foo impl Bar(original_name as alias_name) {
      # The method "original_name" is now available as "alias_name"
    }

Imports use the `import` keyword and `::` is the namespace separator:

    import std::string::ToString

The use of `::` is only valid in the `import` statement, this means that this is
not valid syntax:

    std::string::String.new

This forces one to explicitly state the external modules that are necessary, and
it keeps the syntax simpler.

Comments are created using the `#` sign and run until the end of the line:

    # This is a comment!

## Examples

The venerable Hello World:

    import std::stdout

    stdout.print('Hello, world!')

Concurrent Hello World:

    import std::stdout
    import std::process

    process.spawn {
      stdout.print('Hello from process 1!')
    }

    process.spawn {
      stdout.print('Hello from process 2!')
    }

Checking if a value is true or not:

    import std::stdout

    the_result_of_something.if true: {
      stdout.print('x is true!')
    },
    false: {
      stdout.print('x is false!')
    }

Defining a class:

    class Person {
      # "init" is the constructor method, called whenever you initialize a
      # class.
      fn init(name: String, age: Integer) {
        let @name = name
        let @age = age
      }
    }

Using dynamic typing, simply by leaving out type signatures:

    fn add(left, right) {
      left + right
    }

    add(10, 20)
    add(10.5, 5)
    add('foo', 'bar')

Opening a file and reading data, without blocking the running thread and without
the need of nested callbacks:

    import std::file::File

    let file = File.open('README.md')

    file.read_exact(6) # => "# Inko"

## Requirements

* A UNIX system, Windows is currently not tested/supported

When working on Inko itself you'll also need:

* Rust 1.10 or newer using a nightly build (stable Rust is not supported)
* Cargo

The following dependencies are optional but recommended:

* Make
* Rustup

## Installation (for developers)

The easiest way to install Inko in case you want to hack on it is to first clone
the repository. Once cloned you'll need to build the VM and the compiler.

### Building The VM

To build the VM run the following:

    cd vm
    make

### Building The Compiler

To build the compiler, run the following:

    cd compiler
    make

[immix]: http://www.cs.utexas.edu/users/speedway/DaCapo/papers/immix-pldi-2008.pdf
[error-model]: http://joeduffyblog.com/2016/02/07/the-error-model/
