---
{
  "title": "Visibility"
}
---

Types, constants and methods are private by default. When such a symbol is
private, it's only available to modules that are in the same root namespace.
For example, private symbols defined in `std.foo.baz` are available to
`std.bar.baz`, as both are located in the same `std` root namespace.

## Making types public

For types and constants, making them public is done as follows:

|=
| Type
| Private
| Public
|-
| Constants
| `let Example = 10`
| `let pub Example = 10`
|-
| Fields
| `let @name: Type`
| `let pub @name: Type`
|-
| Classes
| `class Example {}`
| `class pub Example {}`
|-
| Traits
| `trait Example {}`
| `trait pub Example {}`

## Making methods public

For methods the syntax is as follows:

|=
| Type
| Private
| Public
|-
| Immutable
| `fn example {}`
| `fn pub example {}`
|-
| Mutable
| `fn mut example {}`
| `fn pub mut example {}`
|-
| Immutable async
| `fn async example {}`
| `fn pub async example {}`
|-
| Mutable async
| `fn async mut example {}`
| `fn pub async mut example {}`

::: tip
`pub` always comes after the keyword used to define a symbol (e.g. `fn`),
constants or method. The `mut` keyword in turn always comes directly before the
name of the method.
:::

## Processes

The fields and regular (non-async) instance methods of a process are private to
the type, meaning only the process itself can access them:

```inko
class async Cat {
  let @name: String

  fn give_food {}
}

class async Main {
  fn async main {
    let garfield = Cat(name: 'Garfield')

    garfield.name
    garfield.give_food
  }
}
```

If you try to run this program, the following compile-time errors are produced:

```
test.inko:11:5 error(invalid-symbol): the field 'name' can only be used by the owning process
test.inko:12:5 error(invalid-call): the method 'give_food' exists but is private
```

This rule is enforced to ensure no data race conditions are possible as a result
of different processes trying to access and/or mutate the same data.
