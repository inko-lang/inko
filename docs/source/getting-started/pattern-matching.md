# Pattern matching

Inko doesn't have a `switch` expression. Instead, it has a `match` expression.
The `match` expression provides a limited form of pattern matching, similar to
that of [Kotlin's `when` expression](https://kotlinlang.org/docs/reference/control-flow.html#when-expression).

The syntax for pattern matching is as follows:

```inko
match(let variable = input) {
  pattern -> { body }
  else -> { body }
}
```

Here `input` is the input to apply patterns to. The use of parentheses is
required. `let variable = ...` binds the input to the variable `variable`, which
is only available inside the `match` (called a match binding). `pattern -> {
body }` is how a pattern is specified, with `body` being the code to run if the
pattern matches. We call this a case (as in "pattern case"). `else -> { body }`
is a fallback for when no patterns match.

When a pattern is matched, all other patterns are skipped.

!!! tip
    When using pattern matching, the `else` case is required. In the future this
    may no longer be necessary, if the compiler can determine the patterns are
    exhaustive.

## Matching using expressions

Let's say our input is a number. We don't know what the number is, and we only
want to act on the numbers 1, 2, and 3. We can use pattern matching for this as
follows:

```inko
import std::stdio::stdout

def example(input: Integer) {
  match(input) {
    1 -> { stdout.print('one') }
    2 -> { stdout.print('two') }
    3 -> { stdout.print('three') }
    else -> { stdout.print('Something else') }
  }
}
```

Here `1`, `2` and `3` are patterns to test. For the `Integer` type, a pattern
matches if it equals the input number. If the number is not `1`, `2` or `3`, the
above example prints `'Something else'` to STDOUT.

If different expressions specify the same code to run when they match, you can
combine multiple patterns:

```inko
import std::stdio::stdout

def example(input: Integer) {
  match(input) {
    1, 2, 3 -> { stdout.print(input) }
    else -> { stdout.print('Something else') }
  }
}
```

To use a type as a pattern, it must implement the trait `std::operators::Match`.
For some types the implementation is simple: if the pattern equals the input,
it's considered a match. For other types, matching may involve a bit more work.
For example, the `Range` type matches an input if the input is covered by the
range:

```inko
import std::stdio::stdout

match(42) {
  1..100 -> { stdout.print("It's a match!") }
  else -> { stdout.print('No match') }
}
```

Here the output is `"It's a match!"`, because `42` is covered by the range
`1..100`.

!!! note
    When matching input using expressions, the input can't be of type `Any`.

## Matching using types

Besides matching using expressions, we can also match using types:

```inko
import std::stdio::stdout

def example(input: Object) {
  match(input) {
    as Integer -> { stdout.print('An integer') }
    else -> { stdout.print('Something else') }
  }
}
```

Here `as Integer` is a pattern that matches if `input` is an instance of
`Integer`. The type used in these type patterns can be either an object or a
trait:

```inko
import std::conversion::ToString
import std::stdio::stdout

def example(input: Object) {
  match(input) {
    as ToString -> { stdout.print('Something to conver to a String') }
    else -> { stdout.print('Something else') }
  }
}
```

There is one limitation: due to Inko applying type erasure, you can't specify
type parameters in the pattern, as type parameters and their assignments aren't
known at runtime.

Matching input according to types can be combined with a match binding:

```inko
import std::stdio::stdout

def example(input: Object) {
  match(let matched = input) {
    as Integer -> { stdout.print('An integer') }
    else -> { stdout.print('Something else') }
  }
}
```

Within every case, `matched` is typed according to the pattern, removing the
need for explicit type casts:

```inko
import std::stdio::stdout

def example(input: Object) {
  match(let matched = input) {
    as Integer -> {
      matched # typed as Integer
    }
    else -> {
      matched # typed as Object
    }
  }
}
```

## Pattern guards

When specifying patterns, you can add an additional condition to evaluate called
a "pattern" guard. These are specified as follows:

```inko
match(input) {
  foo when bar -> { foo_bar }
  foo when baz -> { foo_baz }
  else -> { quix }
}
```

First the `foo` pattern is applied. If it matches, the `bar` expression is
evaluated. If this expression returns `True`, the condition (`foo_bar`) is
evaluated. If the guard returns `False`, the next pattern is tried.

For a given pattern, only a single pattern guard can be specified. You can't
specify a pattern guard for the `else` pattern.

## Returns types

The return type of a `match` is the return type of the first case specified.

```inko
let result = match(input) {
  1 -> { 10 }
  2 -> { 20 }
  else -> { 30 }
}

result # typed as Integer
```

If additional cases are specified with different return types, the return type
of `match` is `Any`:

```inko
let result = match(input) {
  1 -> { 10 }
  2 -> { 20 }
  else -> { 'foo' }
}

result # typed as Any
```
