# Conditionals

In Aeon conditionals such as `if` and `while` are implemented as methods,
similar to Smalltalk. This cuts down the amount of special keywords and
behaviour required by the virtual machine which in turn should make the language
easier to deal with.

## if

In more imperative languages an if/else statement is usually written as
following:

    if ( condition_one ) {

    else if ( condition_two ) {

    }
    else {

    }

In Aeon there are two methods for dealin with this:

* `if_true`
* `if_false`

Both methods take a closure that is evaluated when a condition is met:

    condition_one
        .if_true { ...  }
        .if_false {
            condition_two.if_true { ... }.if_false { ... }
        }

## while

Traditionally a while loop is written as following:

    while ( some_condition ) {

    }

In Aeon we'd call `while_true` on a closure and provide it with a new closure to
execute while the condition is met:

    let mut x = 0

    { x < 10 }.while_true { x += 1 }

The inverse of `while_true` is `while_false` which works the same except it
calls its closure as long as its receiver evaluates to false:

    luet mut x = 0

    { x > 10 }.while_false { x += 1 }

## match

Pattern matching is usually done in two forms:

1. Using a dedicated construct for pattern matching (e.g. Rust's `match`
   keyword)
2. Using a `switch` statement

For example:

    switch ( some_value ) {
        case 'something':
            ...
            break
    }

In Aeon we'd use the `match` method for this:

    some_value.match('something') { ... }

This method can be used to match both types and instances:

    some_value
        .match(Integer) { ... }
        .match(String)  { ... }

When called on an enum a `match` call must be provided for every variant in the
enum.
