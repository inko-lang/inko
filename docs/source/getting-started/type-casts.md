# Casting types

Sometimes we have a certain type, but want to cast it to another type at
compile-time. This can be done using the `as` keyword, like so:

```inko
expression as TypeName
```

For example, let's say we want to cast a `String` to a `ToString`, a trait
implemented by `String`. We can do this as follows:

```inko
import std::conversion::ToString

'hello' as ToString
```

The inverse is also possible:

```inko
import std::conversion::ToString

let value = 'hello' as ToString

value as String
```

Casting an class instance to a trait instance, or a trait instance to a class
instance, only works if the class implements the trait. Thus, this is not
valid:

```inko
trait NotImplemented {}

'hello' as NotImplemented
```

Casting a trait to another trait is only possible if the target trait is
required by the source trait. For example:

```inko
trait A {}
trait B {}
trait C: B {}

def example1(thing: C) {
  thing as B # This is valid, because C requires B to be implemented
}

def example2(thing: B) {
  thing as A # This is invalid, because A is not required by B
}
```
