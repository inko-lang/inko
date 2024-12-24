---
{
  "title": "Variables"
}
---

Using the `let` keyword we can define two different types of variables: local
variables and constants.

## Local variables

Local variables are defined as follows:

```inko
let number = 42
```

By default, a variable can't be assigned a new value after its definition:

```inko
let number = 42

number = 50 # This produces a compile-time error
```

To allow this, use `let mut` like so:

```inko
let mut number = 42

number = 50 # This is now OK
```

The type of a variable is inferred according to the value assigned to it. A
custom type can be specified as follows:

```inko
let number: Int = 42
```

### Swapping values

Assigning a variable a new value using `=` drops the existing value first, then
assigns the new value to the variable. Using the `:=` we can assign a value and
_return_ the previous value:

```inko
let mut a = 10

a := 20 # This returns `10`
```

This is known as a "swap assignment".

### Drop order

Local variables are dropped in reverse-lexical order:

```inko
let a = foo
let b = bar
```

Here `b` is dropped first, followed by `a`.

## Constants

Constants are defined similar to local variables, except their names start with
a capital letter:

```inko
let NUMBER = 42
```

Unlike local variables, constants can never be assigned a new value. This means
the following is a compile-time error:

```inko
let mut NUMBER = 42
```

Constants can only be defined outside of methods and types, i.e. like so:

```inko
let NUMBER = 42

type Cat {}
```

Constants are permanent values and as such are never dropped.
