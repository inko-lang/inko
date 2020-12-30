# Blocks

In Inko, the collective name of methods, closures and lambdas is "blocks". These
blocks define reusable chunks of code that can take input in the form of
arguments, and produce output by returning or throwing data.

In the [Basic types](basic-types.md) chapter we already took a look at these
blocks, but we explore them in more detail in this chapter.

## Methods

There are a few different types of methods in Inko: instance methods, static
methods, required methods, default methods, and module methods.

A module method is simple a method defined inside a module.

Classes can have static and instance methods, while traits can have required and
default methods. We cover methods for traits and classes in greater detail in
the chapters [Classes](classes.md) and [Traits](traits.md).

## Calling methods

Calling methods is done by sending a message to the thing the method is defined
on, passing zero or more arguments. Typically the message name equals the method
name, though the compiler may decide to optimise method calls away. Parentheses
are only required when passing arguments. Some examples:

```inko
'HELLO'.to_lowercase      # No arguments, so the parentheses can be left out
'HELLO'.starts_with?('H') # One argument, so parentheses are required
greet                     # This is the same as `self.greet`
```

## Calling closures and lambdas

To call a closure or lambda, send the `call` message to it and pass whatever
arguments the closure or lambda needs. For example:

```inko
{ 10 }.call
```

Here we define a closure without any argument and explicit throw/return types,
removing the need for the `do` keyword. Lambdas always require the `lambda`
keyword:

```inko
lambda { 10 }.call
```

## Returning from blocks

The return value of a block is the last expression evaluated:

```inko
{
  10
  20
}.call # => 20
```

If a method doesn't define a return type, it always returns `Nil`.

The `return` keyword is used to return from the surrounding method, and can only
be used in blocks used in a method:

```inko
def example -> Integer {
  Array.new(10, 20, 30).each do (number) {
    return number
  }
}

example # => 10
```

A `return` at the module-level is invalid:

```inko
def foo {}

return # This is invalid, because we are not inside a method
```

If you use `return` without a value, it will return `Nil`. If the surrounding
method specifies a return type, the returned value must be compatible with this
type. If a method doesn't define a return type, you can only `return` a `Nil`.

## Arguments

Methods can take zero or more arguments. Arguments can be required or optional.
Each method can also have a single rest argument.

We define required arguments as follows:

```inko
def greet(name: String) {}  # A module method
do (name: String) {}        # A closure
lambda (name: String) {}    # A lambda
```

Here `name` is the name of the argument, and `String` is its type. To define an
optional argument, provide a default value for the argument:

```inko
def greet(name = 'Alice') {}
do (name = 'Alice') {}
lambda (name = 'Alice') {}
```

When specifying a default value, you can still specify an explicit argument
type:

```inko
def greet(name: String = 'Alice') {}
do (name: String = 'Alice') {}
lambda (name: String = 'Alice') {}
```

In this case the explicit type will be used as the argument type. It's an error
to assign a default value that's not compatible with the explicit type.

A rest argument is used for storing any excessive arguments passed to the
method. A method can only have a single rest argument, and it must be the last
argument:

```inko
def greet(name: String, *other: String) {}
do (name: String, *other: String) {}
lambda (name: String, *other: String) {}
```

Here `other` is the rest argument. We can call this method like so:

```inko
greet('Alice', 'Bob', 'Eve')
```

When using a closure or lambda, you can leave out the argument type even for
required types. When leaving these types out, the compiler will try to infer the
types for you. For example:

```inko
Array.new(10, 20, 30).each do (number) {}
```

Here the compiler knows that the type of `number` is `Integer`, because the
values in the `Array` are of this type. Because of this, you can leave out the
explicit argument type.

!!! tip
    Support for inferring closure/lambda arguments types is limited, and only
    works when the block is directly passed as an argument. This may be improved
    upon in the future.

## Mutable arguments

A mutable argument is an argument that can be assigned a new value, provided
this value is compatible with the argument type. To make an argument mutable,
add the `mut` keyword before the argument name:

```inko
def foo(mut number = 10) {
  number = 20
}
```

This works for all argument types.

## Keyword arguments

When passing arguments along with a message, you can use either positional
arguments or keyword arguments. Keyword arguments are useful when the meaning of
the argument itself is not clear. Take this method for example:

```inko
def withdraw(euros: Integer) {}
```

Using positional arguments for this method looks like this:

```inko
withdraw(10)
```

When reading this line, it may not be clear what the value `10` means. Using
keyword arguments, this becomes more clear:

```inko
withdraw(euros: 10)
```

Keyword arguments also let you pass arguments out of order. Take this method for
example:

```inko
def calculate(a: Integer, b: Integer, c: Integer) -> Integer {
  a + b - c
}
```

Using keyword arguments, we can call it like so:

```inko
calculate(a: 1, b: 2, c: 3)
calculate(c: 3, a: 1, b: 2)
calculate(b: 2, c: 3, a: 1)
```

These three lines all translate to the same code and result. This is useful for
optional arguments, as you can leave out arguments you don't want to set
yourself. For example:

```inko
def bake(ingredient: String, celcius = 200, minutes = 60) {}

bake(ingredient: 'Bread', minutes: 120)
```

When using keyword arguments, two things should be kept in mind. First, they must
come after any positional arguments. This means the following is not valid:

```inko
bake(ingredient: 'Bread', 250)
```

Second, you can't use keyword arguments to address rest arguments. This means
the following is invalid:

```inko
def greet(*names: String) {}

greet(names: 'Alice', names: 'Bob')
```

In practise this means it's best to only use rest arguments when the meaning of
any positional arguments is clear on their own, or when the method _only_
defines a rest argument.

Closures and lambdas of course also support keyword arguments:

```inko
do (number: Integer) { number }.call(number: 42)
```

## Trailing block arguments

When passing a block as the last argument, you can place it outside the
parentheses. When passing only a block as the argument, you can leave out the
parentheses. For example:

```inko
Array.new(10, 20, 30).each do (number) {}
```

Here `each` only takes a single argument, so no parentheses need to be
specified. If it did take an argument, we would write the following:

```inko
Array.new(10, 20, 30).each(something) do (number) {}
```

Passing blocks this way is _only_ supported if the last argument is a block, and
it only works for a single block. This means the following is invalid:

```inko
Array.new(10, 20, 30).each do (number) {}, do (foo) {}
```

## Throw types

A method can throw a value. If so, it must specify the type of the value thrown
in its signature. This is done as follows:

```inko
def example !! Integer {}
```

Here the `example` method states that it will throw a value of type `Integer`.
For closures and lambdas, the syntax is the same:

```inko
do !! Integer {}
lambda !! Integer {}
```

How to throw values and handle errors is covered separately.

## Return types

When a method doesn't specify a return type explicitly, it will return `Nil` and
its return type is inferred as `Nil`. You can specify an explicit return type as
follows:

```inko
def example -> String {}
```

Here the method states that it returns an instance of the `String` type. The
syntax for closures and lambdas is the same:

```inko
do !! Integer {}
lambda !! Integer {}
```

When adding an explicit throw and return type to a block, the throw type must
come first:

```inko
def example !! Integer -> String {}
do !! Integer -> String {}
lambda !! Integer -> String {}
```
