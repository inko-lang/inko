# Optional values

Option values are created using the `Option` type, defined in the `std::option`
module. The `Option` type is an object that can either wrap a value, in which
case it's called a "Some", or signal the lack of a value, in which case it's
called a "None". You can create such values as follows:

```inko
Option.some(10)
Option.none
```

The type signature of an `Option` is `Option!(T)` where `T` is the type that's
wrapped when present. Since the `Option` type is such a commonly used type, Inko
provides syntax sugar to safe you some typing: `?T`. It's recommended that you
use `?T` instead of `Option!(T)`.

## Getting Option values

An `Option` is its own object that wraps a value (if present). To use that
value, you must explicitly get it from the wrapping `Option`. You can do this
using `Option.get`:

```inko
let x = Option.some(10)

x.get # => 10
```

This method panics when used on a "None". There are two methods you can use to
provide a default value, in case the `Option` is a "none":

* `Option.get_or`
* `Option.get_or_else`

The method `get_or` takes an argument, evaluates it, and returns it if the
`Option` is a "None". The method `get_or_else` takes a closure, and evaluates it
when the `Option` is a "None":

```inko
Option.none.get_or(0)         # => 0
Option.none.get_or_else { 0 } # => 0
```

If the default value is already defined (e.g. it's just a variable), it's
recommended to use `get_or`. If the default should only be evaluated if the
`Option` is a "None", the use of `get_or_else` is recommended. For example, if
the default value is the contents of a file that has yet to be read, it's best
to use `get_or_else` instead of `get`.

## Mapping Option values

Sometimes an `Option` needs to be converted into another `Option`, such as
converting a `Option!(Integer)` into an `Option!(String)`. This can be done
using `Option.map` and `Option.then`.

`Option.map` takes a closure that returns a value. If the `Option` is a "Some",
`Option.map` returns a new `Option` wrapping the returned value. If the `Option`
is a "None", another "None" is returned:

```inko
Option.some(10).map do (num) { num * 2 }      # => Option.some(20)
Option.none.map do (num: Integer) { num * 2 } # => Option.none
```

`Option.then` is similar, except its closure returns an `Option`. For a "Some",
that `Option` is returned, otherwise a "None" is returned:

```inko
Option.some(10).then do (num) { Option.some(num * 2) }      # => Option.some(20)
Option.none.then do (num: Integer) { Option.some(num * 2) } # => Option.none
```

## Comparing Option values

The `Option` type implements the `Equal` trait, allowing you to compare it to
other `Option` objects that wrap a value of the same type:

```inko
Option.some(10) == Option.some(10) # => True
Option.some(10) == Option.some(20) # => False
Option.some(10) == Option.none     # => False
```

## Option truthyness

An `Option` is considered to be True if it's a "Some", otherwise it's considered
as False:

```inko
Option.some(10).if(true: { 10 }, false: { 20 }) # => 10
Option.none.if(true: { 10 }, false: { 20 })     # => 20
```
