# Variables and fields

Variables are defined using the `let` keyword. For an overview of the syntax,
refer to [Defining variables](syntax.md#defining-variables) in the syntax guide.

Inko infers the types of variables based on their values:

```inko
let a = 10 # `a` is inferred as `Int`
```

For more information about type inference, refer to the [Type
inference](types.md#type-inference) section.

Fields are defined using the `let` keyword in a class definition:

```inko
class Person {
  let @name: String
}
```

## Assigning variables and fields

Variables and fields are assigned new values using `=`. For variables this
requires the variable to be mutable:

```inko
let a = 10
let mut b = 10

a = 20 # Not OK, `a` isn't defined as mutable
b = 20 # This is OK
```

For fields the surrounding method must be mutable:

```inko
class Person {
  let @name: String

  fn foo {
    @name = 'Alice' # Not OK as `foo` is not a mutable method
  }

  fn mut foo {
    @name = 'Alice' # OK
  }
}
```

When a variable or field is assigned a new value, its old value is dropped.
Assignments always return `nil`.

Inko also supports swapping of values using `:=`, known as a "swap assignment".
This works the same as regular assignments, except the old value is returned
instead of dropped:

```inko
let mut a = 10

a = 20 # This returns `10`
```

This also works for fields:

```inko
class Person {
  let @name: String

  fn replace_name(new_name: String) -> String {
    @name := new_name
  }
}
```

## Ownership

When a value is assigned to a variable, the value is moved into that variable.
If the value is owned this means the original variable (if there was any) is no
longer available. If the value is a reference, the variable is given a new
reference, allowing you to continue using the old variable:

```inko
let a = [10]
let b = a

a.pop # Invalid, as `a` is moved into `b`
```

## Field ownership

The type fields are exposed as depends on the kind of method the field is used
in. If a method is immutable, the field type is `ref T`. If the method is
mutable, the type of a field is instead `mut T`; unless it's defined as a `ref
T`:

```inko
class Person {
  let @name: String
  let @grades: ref Array[Int]

  fn foo {
    @name   # => ref String
    @grades # => ref Array[Int]
  }

  fn mut foo {
    @name   # => mut String
    @grades # => ref Array[Int]
  }

  fn move foo {
    @name   # => String
    @grades # => ref Array[Int]
  }
}
```

If a method is marked as moving using the `move` keyword, you can move fields
out of their owner, and the fields are exposed using their original types (i.e.
`@name` is exposed as `String` and not `mut String`):

```inko
class Person {
  let @name: String

  fn move into_name -> String {
    @name
  }
}
```

When moving a field, the remaining fields are dropped individually and the owner
of the moved field is partially dropped. It's a compile-time error to use the
same field or `self` after a field is moved. You also can't capture any fields
or `self` from the owner the field is moved out of.

If a type defines a custom destructor, its fields can't be moved in a moving
method.

## Drop semantics

When exiting a scope, any variables defined in this scope are dropped in
reverse-lexical order. This means that if you define `a` and then `b`, `b` is
dropped before `a`.

When using `return` or `throw`, all variables defined up to that point are
dropped in the same reverse-lexical order.

## Conditional moves and loops

If a variable is dropped conditionally, it's not available afterwards:

```inko
let a = [10]

if something {
  let b = a
}

# `a` _might_ be moved at this point, so we can't use it anymore.
```

The same applies to loops: if a variable is moved in a loop, it can't be used
outside the loop:

```inko
let a = [10]

loop {
  let b = a
}
```

Any variable defined outside of a loop but moved inside the loop _must_ be
assigned a new value before the end of the loop. This means the above code is
incorrect, and we have to fix it like so:

```inko
let mut a = [10]

loop {
  let b = a

  a = []
}
```

We can do the same for conditions:

```inko
let mut a = [10]

if condition {
  let b = a

  a = []
}

# `a` can be used here, because we guaranteed it always has a value at this
# point
```

If a value is moved in one branch of a condition, it's still available in the
other branches:

```inko
let a = [10]

# This is fine, because only one branch ever runs.
if foo {
  let b = a
} else if bar {
  let b = a
}
```

This also applies to pattern match expressions.

To handle dropping of conditionally moved variables, Inko uses hidden variables
called "drop flags". These are created whenever necessary and default to `true`.
When a variable is moved its corresponding drop flag (if any) is set to `false`.
When it's time to drop the variable, the compiler inserts code that checks the
value of this flag and only drops the variable if the value is still `true`.
This means that this:

```inko
let a = [10]

if condition {
  let b = a
}
```

Is more or less the same as this:

```inko
let a = [10]
let a_flag = true

if condition {
  a_flag = false

  let b = a
}

if a_flag {
  drop(a)
}
```
