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

The compiler emits a warning for any unused local variables. To silence such
warnings, prefix the variable name with an underscore:

```inko
let _no_warning_for_me = 42
```

It's also possible to use _just_ an underscore as the name:

```inko
let _ = 42
```

The difference between the two is that if the name is _just_ an underscore, the
assigned value is dropped right away. If the variable name merely starts with an
underscore (e.g. `_no_warning_for_me`) the value is dropped at the end of the
surrounding scope.

### Pattern matching

`let` doesn't just _let_ (we're not sorry for that pun) you define variables, it
supports pattern matching too. In fact, `let` expressions are compiled to
`match` expressions, just using different syntax. You can find out more about
this [here](pattern-matching#pattern-matching-with-let).

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
