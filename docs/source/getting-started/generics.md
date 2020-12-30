# Generics

Inko supports generic classes, traits and blocks. Examples of built-in generic
types are `Array`, `Map`, `Pair`, and `Range`.

## Generic classes

A generic class is defined as follows:

```inko
class List!(T) {}
```

Here we define a `List` class with a single type parameter: `T`. A list of type
parameters starts with `!(` and ends with a `)`. You can use type parameters in
signatures:

```inko
class List!(T) {
  def push(value: T) {
    # This is OK
  }

  def foo {
    T # This is not, because T isn't a type that exists at runtime.
  }
}
```

When defining a type parameter, you can specify a list of traits that types
assigned to the type parameter must implement:

```inko
trait Foo {}
trait Bar {}

class List!(T: Foo + Bar) {}
```

Here any type assigned to `T` must implement the traits `Foo` and `Bar`.

If you want to define multiple type parameters, separate them with a comma:

```inko
class List!(A, B, C) {}
```

Here's how you'd define a generic list type that supports pushing and popping of
values:

```inko
class List!(T) {
  @values: Array!(T)

  static def new -> Self {
    Self { @values = Array.new }
  }

  def push(value: T) {
    @values.push(value)
  }

  def pop -> ?T {
    @values.pop
  }
}
```

When using a generic class in a type signature, you must specify the types to
assign to the class' type parameters. In case of our `List` type, that means
this isn't valid:

```inko
def foo(list: List) {}
```

Instead we have to write something like this:

```inko
def foo(list: List!(Integer)) {}
```

When reopening a generic class, you don't need to specify its type parameters
again:

```inko
impl List {}
```

## Generic traits

Generic traits are defined the same way as generic classes:

```inko
trait ToList!(T) {}
```

When implementing a generic trait, you must specify the types to assign to the
trait's type parameters. That means the following is invalid:

```inko
impl ToList for String {}
```

Instead, you'd have to write the following:

```inko
impl ToList!(String) for String {}
```

A trait (including generic traits) can only be implemented once, so the
following is invalid:

```inko
impl ToList!(Integer) for String {}
impl ToList!(String) for String {}
```

## Generic blocks

Methods, closures and lambdas can also be generic. The syntax is as follows:

```inko
do !(T)(value: T) {}         # A generic closure
lambda !(T)(value: T) {}     # A generic lambda
def example!(T)(value: T) {} # A generic method
```

When calling a generic block, its type parameters are inferred based on how the
block is used:

```inko
def to_array!(T)(value: T) -> Array!(T) {
  Array.new(value)
}

to_array(10) # Here the compiler will infer T as an Integer
```

If for whatever reason you need to specify a type parameter explicitly, you can
do so as follows:

```inko
def to_array!(T)(value: T) -> Array!(T) {
  Array.new(value)
}

to_array!(Integer)(10)
```

## Type erasure

Inko applies type erasure. This means that at runtime a `Array!(Integer)` and
`Array!(String)` use the same underlying type and methods. This also means you
can't determine, at runtime, if an `Array` is an `Array!(Integer)` or something
else; at least not without check its values.
