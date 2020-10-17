# Error handling

Inko uses exceptions for error handling, albeit in a somewhat different fashion
compared to other languages. Inko's error handling is inspired by the article
["The Error Model"](http://joeduffyblog.com/2016/02/07/the-error-model/) by Joe
Duffy.

## Throwing in a method

When a method throws an error, it must define the type to throw in its
signature. A method that throws an `Error` would be defined as follows:

```inko
def example !! Error {
  # ...
}
```

Here `!! Error` means "This method will throw an `Error`". The type thrown can
be an object or a trait, but _only one type_ can be thrown. This simplifies
error handling, as a caller of a method only needs to handle a single error
type.

If a method defines a type to throw, it _must_ at some point actually throw this
type. It's a compile-time error to define a method with a throw type, without it
actually throwing:

```inko
def example !! Error -> Integer {
  # This would not compile, since we never throw an error.
  10
}
```

Throwing an error is done using the `throw` keyword:

```inko
def withdraw_money(amount: Integer) !! String -> Integer {
  amount.positive?.if(
    true: { amount },
    false: { throw "You can't withdraw a negative amount of money!" }
  )
}
```

If `amount` is greater than zero, we just return the value, otherwise we throw a
`String`.

Note that the `throw` keyword will throw from the surrounding method, much like
how the `return` keyword returns from the surrounding method.

## Sending messages that may throw

To send a message that throws, you must use the `try` or `try!` keyword. Both
keywords will run an expression, but both will respond differently to an error.
When using `try`, the error will be re-thrown:

```inko
def withdraw_money(amount: Integer) !! String -> Integer {
  amount.positive?.if(
    true: { amount },
    false: { throw "You can't withdraw a negative amount of money!" }
  )
}

def transfer_money(amount: Integer) !! String -> Integer {
  let amount = try withdraw_money(amount)

  transfer_to_other_account(amount)
}
```

Here `try withdraw_money(amount)` re-throws any errors thrown by
`withdraw_money`. When using `try` like this, all previous rules apply as well.
This means the following code is invalid, because `transfer_money` doesn't
define a type to throw:

```inko
def withdraw_money(amount: Integer) !! String -> Integer {
  amount.positive?.if(
    true: { amount },
    false: { throw "You can't withdraw a negative amount of money!" }
  )
}

def transfer_money(amount: Integer) -> Integer {
  let amount = try withdraw_money(amount)

  transfer_to_other_account(amount)
}
```

We can handle errors by using the form `try EXPR else (error) ELSE`, with `EXPR`
being the expression that may throw, `error` being a local variable to store the
error in, and `ELSE` being the expression(s) to run when an error is thrown.
Both the try and else expressions can be wrapped in curly braces, but the try
expression can only be a single expression:

```inko
# Valid
try foo else (error) bar

# Also valid
try { foo } else (error) { bar }

# This is not valid, because the `try` body can only contain a single
expression.
try {
  foo
  bar
} else (error) {
  bar
}
```

Using a `try` with an `else`,  we can change the above example to the following:

```inko
def withdraw_money(amount: Integer) !! String -> Integer {
  amount.positive?.if(
    true: { amount },
    false: { throw "You can't withdraw a negative amount of money!" }
  )
}

def transfer_money(amount: Integer) -> Integer {
  let amount = try withdraw_money(amount) else 0

  transfer_to_other_account(amount)
}
```

In this case we just ignore any errors thrown by `withdraw_money` and assign
`amount` to `0` instead. If we wanted to do something with the error, we would
change our code to the following:

```inko
def withdraw_money(amount: Integer) !! String -> Integer {
  amount.positive?.if(
    true: { amount },
    false: { throw "You can't withdraw a negative amount of money!" }
  )
}

def transfer_money(amount: Integer) !! String -> Integer {
  let amount = try {
    withdraw_money(amount)
  } else (error) {
    throw 'Encountered the following error: ' + error
  }

  transfer_to_other_account(amount)
}
```

When storing the error in a variable you do not need to specify its type, as the
compiler will infer this for you.

Sometimes there is no sensible way of handling an error. For example, we may
need to open a read-only file that we can't create during runtime. For these
cases you can use the `try!` keyword:

```inko
def withdraw_money(amount: Integer) !! String -> Integer {
  amount.positive?.if(
    true: { amount },
    false: { throw 'You can not withdraw a negative amount of money!' }
  )
}

def transfer_money(amount: Integer) -> Integer {
  let amount = try! withdraw_money(amount)

  transfer_to_other_account(amount)
}
```

When using `try!`, any error encountered in the expression will result in a
panic.

## Catching errors from multiple expressions

Inko does not support wrapping multiple expressions using the `try` or `try!`
keywords. This means code such as this is invalid:

```inko
try {
  let amount = withdraw_money(10)
  let transferred = transfer_money(amount)

  # ...
}
```

The choice to not support this is deliberate. By limiting the `try` and `try!`
keywords to only a single expression, error handling becomes more fine grained.
This in turn makes debugging and refactoring easier, as a change in the error
API will not require you to change hundreds of lines in a `try` expression.

The block provided to the `else` keyword _can_ contain multiple expressions.

## Panics

Panics are critical errors that by default stop the entire program. These kind
of errors should only be used when there is nothing that can be done at runtime.
Examples of operations that may trigger a panic include (but are not limited
to):

* Dividing by zero.
* Trying to allocate new memory when the system doesn't have any remaining
  memory.
* Trying to set the value of an out of bounds byte array index.

To illustrate, take the following program:

```inko
import std::byte_array::ByteArray

let bytes = ByteArray.new(10, 20)

bytes[3] = 10
```

When executed it will panic with the following output:

```
Stack trace (the most recent call comes last):
  0: "test.inko", line 5, in "main"
  1: "runtime/std/byte_array.inko", line 240, in "[]="
Process 0 panicked: Byte array index 3 is out of bounds
```

The use of panics for critical errors greatly reduces the amount of exceptions
you need to handle, making error handling more pleasant.

If you want a panic to only stop the process that triggered it, you'll need to
register a panic handler using `std::process.panicking`. This is a block that
will be executed whenever a panic is triggered, after which the process will
stop. The argument passed to this block is an error message as a `String`.

If a process does not define its own panic handler, the global panic handler
will be used. This panic handler can be overwritten using `std::vm.panicking`:

```inko
import std::vm
import std::stdio::stderr

vm.panicking {
  stderr.print('oops, we ran into a panic!')
}
```

Note that you can't restore the global panic handler after you have redefined
it. Also keep in mind that if you overwrite the global panic handler, Inko will
_not_ stop the program for you, as this is done by the default global handler.
This means that if you still want to stop the program, you have to do so
manually using `std::vm.exit`:

## Panics versus exceptions

Exceptions should be used for everything that you expect to occur during
runtime. This includes network timeouts, file permission errors, input
validation errors, and so on.

Panics should _only_ be used for critical errors that should not occur at
runtime in a well written program.
