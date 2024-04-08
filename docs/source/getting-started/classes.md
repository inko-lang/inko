---
{
  "title": "Classes"
}
---

Classes are used for storing state used by methods. One such class we've seen
many times so far is the `Main` class, which defines the main process to run.

Classes are defined using the `class` keyword like so:

```inko
class Person {

}
```

Here `Person` is the name of the class.

## Fields

Fields are defined using the `let` keyword in a `class` body:

```inko
class Person {
  let @name: String
  let @age: Int
}
```

Here we've defined two fields: `name` of type `String`, and `age` of type `Int`.
The `@` symbol isn't part of the name, it's just used to disambiguate the syntax
when referring to fields. Using fields uses the same syntax:

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

We don't need to define getter and setter methods for fields though, as Inko
allows you to get and set field values directly:

```inko
let alice = Person { @name = 'Alice', @age = 42 }

alice.name # => 'Alice'
alice.name = 'Bob'
alice.name # => 'Bob'
```

The type fields are exposed as depends on the kind of method the field is used
in. If a method is immutable, the field type is `ref T`. If the method is
mutable, the type of a field is instead `mut T`, unless it's defined as a
`ref T`:

```inko
class Person {
  let @name: String
  let @grades: ref Array[Int]
  let @friends: Array[ref Person]

  fn foo {
    @name    # => String
    @grades  # => ref Array[Int]
    @friends # => ref Array[ref Person]
  }

  fn mut foo {
    @name    # => String
    @grades  # => ref Array[Int]
    @friends # => mut Array[ref Person]
  }

  fn move foo {
    @name    # => String
    @grades  # => ref Array[Int]
    @friends # => Array[ref Person]
  }
}
```

If a method takes ownership of its receiver, you can move fields
out of `self`, and the fields are exposed using their original types (i.e.
`@name` is exposed as `String` and not `mut String`).

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
Person { @name = 'Alice', @age = 42 }
```

Here we create a `Person` instance with the `name` field set to `'Alice'`, and
the `age` field set to `42`.

Sometimes creating an instance of a class involves complex logic to assign
values to certain fields. In this case it's best to create a static method to
create the instance for you. For example:

```inko
class Person {
  let @name: String
  let @age: Int

  fn static new(name: String, age: Int) -> Person {
    Person { @name = name, @age = age }
  }
}
```

Of course nothing complex is happening here, instead we're just trying to
illustrate what using a static method for this might look like.

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

Here we've defined a `Letter` enum with three possible cases: `A`, `B`, and `C`.
We can create instances of these cases as follows:

```inko
Letter.A
Letter.B
Letter.C
```

The cases in an enum support arguments, allowing you to store data in them
similar to using regular classes with fields:

```inko
class enum OptionalString {
  case None
  case Some(String)
}
```

We can then create an instance of the `Some` case as follows:

```inko
OptionalString.Some('hello')
```

Unlike other types of classes, you can't use the syntax `OptionalString { ... }`
to create an instance of an enum class.

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
Cat { @name = 'Garfield' }
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
