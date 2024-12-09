---
{
  "title": "Classes"
}
---

Classes are data structures that can store state and define methods. One such
class we've seen many times so far is the `Main` class, which defines the main
process to run.

Classes are defined using the `class` keyword like so:

```inko
class Person {

}
```

Here `Person` is the name of the class. Inside the curly braces the fields and
methods of a class are defined.

## Fields

Fields are defined using the `let` keyword:

```inko
class Person {
  let @name: String
  let @age: Int
}
```

Here we've defined two fields: `name` of type `String`, and `age` of type `Int`.
The `@` symbol isn't part of the name, it's just used to disambiguate the syntax
when referring to fields. Referring to fields uses the same syntax:

```inko
class Person {
  let @name: String
  let @age: Int

  fn name -> String {
    @name
  }
}
```

Here the `name` method just returns the value of the `@name` field.

The type fields are exposed as depends on the kind of method the field is used
in. If a method is immutable, the field type is `ref T`. If the method is
mutable, the type of a field is instead `mut T`, unless it's defined as a
`ref T`. If the field's type is a value type such as `Int` of `String`, the type
is exposed as-is:

```inko
class Person {
  let @name: String
  let @age: Int
  let @grades: ref Array[Int]
  let @friends: Array[ref Person]

  fn foo {
    @name    # => String
    @age     # => Int
    @grades  # => ref Array[Int]
    @friends # => ref Array[ref Person]
  }

  fn mut foo {
    @name    # => String
    @age     # => Int
    @grades  # => ref Array[Int]
    @friends # => mut Array[ref Person]
  }

  fn move foo {
    @name    # => String
    @age     # => Int
    @grades  # => ref Array[Int]
    @friends # => Array[ref Person]
  }
}
```

If a method takes ownership of its receiver, you can move fields out of `self`,
and the fields are exposed using their original types (i.e. `@nagrades` is
exposed as `Array[Int]` and not `mut Array[Int]`).

When moving a field, the remaining fields are dropped individually and the owner
of the moved field is partially dropped. If a type defines a custom destructor,
a `move` method can't move the fields out of its receiver.

## Swapping field values

Similar to local variables, `:=` can be used to assign a field a new value and
return its old value, instead of dropping the old value:

```inko
class Person {
  let @name: String

  fn mut replace_name(new_name: String) -> String {
    @name := new_name
  }
}
```

## Initialising classes

An instance of a class is created as follows:

```inko
class Person {
  let @name: String
  let @age: Int
}

Person(name: 'Alice', age: 42)
```

Here we create a `Person` instance with the `name` field set to `'Alice'`, and
the `age` field set to `42`. We can also use positional arguments, in which case
the order of arguments must match the order in which fields are defined:

```inko
Person('Alice', 42)
```

::: tip
It's recommended to avoid the use of positional arguments when a class defines
more than one field. This ensures that if the order of fields changes, you don't
need to update every line of code that creates an instance of the class.
:::

The fields of an instance can be read from and written to directly, meaning we
don't need to define getter and setter methods:

```inko
let alice = Person(name: 'Alice', age: 42)

alice.name # => 'Alice'
alice.name = 'Bob'
alice.name # => 'Bob'
```

Sometimes creating an instance of a class involves complex logic to assign
values to certain fields. In this case it's best to create a static method to
create the instance for you. For example:

```inko
class Person {
  let @name: String
  let @age: Int

  fn static new(name: String, age: Int) -> Person {
    Person(name: name, age: age)
  }
}
```

Of course nothing complex is happening here, instead we're just trying to
illustrate what using a static method for this might look like.

## Reopening classes

A class can be reopened using the `impl` keyword like so:

```inko
class Person {
  let @name: String
  let @age: Int
}

impl Person {
  fn greet -> String {
    'Hello ${@name}'
  }
}
```

When reopening a class, only new methods can be added to the class. It's a
compile-time error to try to add a field or overwrite an existing method.

## Enums

Inko also has "enum classes", created using `class enum`. Enum classes are used
to create sum types, also known as enums:

```inko
class enum Letter {
  case A
  case B
  case C
}
```

Here we've defined a `Letter` enum with three possible cases (also known as
"constructors"): `A`, `B`, and `C`. We can create instances of these cases as
follows:

```inko
Letter.A
Letter.B
Letter.C
```

The constructors in an enum support arguments, allowing you to store data in
them similar to using regular classes with fields:

```inko
class enum OptionalString {
  case None
  case Some(String)
}
```

We can then create an instance of the `OptionalString.Some` as follows:

```inko
OptionalString.Some('hello')
```

Unlike other types of classes, you can't use the syntax `OptionalString(...)`
to create an instance of an enum class.

## Inline types

When defining a class using the `class` keyword, instances of such a class are
allocated on the heap and accessed through a pointer. While this gives you the
greatest amount of flexibility, heap allocations can be expensive if done
frequently. To work around this, you can define a stack allocated type using the
`inline` keyword:

```inko
class inline Person {
  let @name: String
  let @age: Int
}
```

The `inline` keyword can also be combined with the `enum` keyword:

```inko
class inline enum Letter {
  case A
  case B
  case C
}
```

Similar to regular classes, you can borrow instances of `inline` classes and
still move the instance you are borrowing from around, without the need for
compile-time borrow checking as is necessary in
[Rust](https://www.rust-lang.org/). This is made possible (and safe) by
_copying_ the stack allocated data upon borrowing it, and then increasing the
borrow count for any interior heap values.

This approach does mean that `inline` classes come with some restrictions and
caveats:

- Fields can't be assigned new values, as copying the stack data means the
  assignment would only be visible to the copy used by the assignment.
- Borrowing interior heap data means that if an `inline` class stores 8 heap
  allocated values, borrowing the `inline` class results in 8 borrow count
  increments.
- Recursive `inline` types aren't supported, and the compiler will emit a
  compile-time error when encountering such types. This restriction exists
  because the compiler must be able to determine the size of an `inline` type,
  which isn't possible if it's recursive.
- Since instances of `inline` types are stored on the stack, programs may
  consume more stack space, though this is unlikely to pose an actual problem.
- `inline` types can't be cast to traits.

## Value types

Besides supporting regular stack allocated types, Inko also supports stack
allocated immutable value types. Such types are defined using the `copy` keyword
when defining a class:

```inko
class copy Number {
  let @value: Int
}
```

The `copy` modifier is also available for enums:

```inko
class copy enum Example {
  case A(Int)
  case B(Float)
}
```

When using this modifier, instances of the class are allocated on the stack and
become _immutable_ value types that are copied upon a move. For the above
`Number` example that means the memory representation is the same as that of the
`Int` type.

Because these types are immutable, it's not possible to assign fields new values
or define `fn mut` methods on such types. Instead, the approach to "mutation" is
to return a new copy of the instance containing the appropriate changes. For
example:

```inko
class copy Number {
  let @value: Int

  fn increment(amount: Int) -> Number {
    Number(@value + amount)
  }
}
```

Classes defined using the `copy` modifier can only store instances of the
following types:

- `Int`, `Float`, `Bool`, `Nil`
- Other `copy` types

Most notably, `String` values can't be stored in a `copy` type as `String` uses
atomic reference counting. This means the following definition is invalid:

```inko
class copy InvalidType {
  let @value: Array[Int] # Array[Int] isn't an `inline` type
}
```

The same restriction applies to generic type parameters:

```inko
class copy Box[T] {
  let @value: T
}

Box([10]) # T requires a `copy` type, but `Array[Int]` isn't such a type
```

Classes defined using the `copy` keyword can't implement the `std.drop.Drop`
trait and thus can't define custom destructors. If you need to implement this
trait, you'll have to use the `inline` keyword instead.

Similar to `inline` types, `copy` types can't be cast to traits.

## Inline vs heap types

With Inko supporting both heap and stack allocated types, one might wonder:
when should I use the `inline` or `copy` modifier when defining a class? To
answer this question, ask yourself the following questions:

- Do you need to assign a field a new value?
- Is the type a recursive type?
- Will the type be storing many heap allocated values (e.g. more than 8)?
- Is the type large (i.e. 128 bytes or more)?
- Do you want to be able to cast the type to a trait?

If the answer to any of these questions is "Yes", you'll want to use a regular
heap allocated `class`. If the answer to each question is "No", then you need to
decide between using the `inline` and `copy` modifier. The decision between
these two modifiers is a bit easier: if you only intend to store `copy` types
(`Int`, `Float`, etc) and don't intend to mutate the type, it's best to use
`class copy`, while for all other cases it's best to use `class inline`.

## Processes

Processes are defined using `class async`, and creating instances of such
classes spawns a new process:

```inko
class async Cat {

}
```

Just like regular classes, async classes can define fields using the `let`
keyword:

```inko
class async Cat {
  let @name: String
}
```

Creating instances of such classes is done the same way as with regular classes:

```inko
Cat(name: 'Garfield')
```

Processes can define `async` methods that can be called by other processes:

```inko
class async Cat {
  let @name: String

  fn async give_food {
    # ...
  }
}
```

## Drop order

When dropping an instance of a class with fields, the fields are dropped in
reverse-definition order:

```inko
class Person {
  let @name: String
  let @age: Int
}
```

When dropping an instance of this class, `@age` is dropped before `@name`.

When dropping an `enum` with one or more cases that store data, the data stored
in each case is dropped in reverse-definition order:

```inko
class enum Example {
  case Foo(Int, String)
  case Bar
}
```

When dropping an instance of `Example.Foo`, the `String` value is dropped before
the `Int` value.
