# Methods

Methods are routines defined on an object be it a class or an instance. A method
has a name, a list of arguments (optional) and an optional return type. If no
return type is specified it's assumed the method returns void (= nothing).

A method is defined using the `def` keyword followed by a name and the argument
list:

    def some_method() {

    }

The parenthesis are required, even when the method has no arguments. When
calling a method the parenthesis are not required, although it's good practise
to use them when specifying one or more arguments:

    some_method 10, 20  # technically correct, but bad practise
    some_method(10, 20) # better

## Returns

If a method has a return value you can specify the return type in the method
definition using the `->` symbol:

    def some_method() -> Integer {
        return 10
    }

If a method returns a dynamic value you can use the `dynamic` keyword. This is
not an actual type, it's simply a hint for the compiler/virtual machine:

    def some_method() -> dynamic {
        something
          .if_true  { 10 }
          .if_false { 'hello' }
    }

One alternative to this would be to specify the return type as being `Object`
instead. However, by using `dynamic` the compiler knows not to check any
method calls called on a return value. For example, when using `dynamic` the
following is valid:

    def some_method() -> dynamic { ... }

    some_method().example()

However, this would not be valid:

    def some_method() -> Object { ... }

    some_method().example()

Because Object does not define "example" this would lead to a compiler error.

It's a compiler error to define a return type but not return anything. Not
specifying a return type will result in implicit return values being ignored:

    def some_method() { 10 } # return value ignored

The explicit usage of the `return` keyword in a method without a return type is
an error:

    def some_method() { return 10 } # error

Return values are implicit, although you can also use the `return` keyword to
return earlier than usual. Both the following examples are valid:

    def some_method() -> Integer {
        10
    }

    def some_method() -> Integer {
        return 10
    }

When using implicit return values the value of the last expression in a method
body is treated as the return value.

The rationale for implicit return values is to make it more pleasant to use
closures. If only explicit returns were supported we'd have to write this:

    def some_method(value: Integer) {
        return (value > 10)
            .if_true  { return 'Yes!' }
            .if_false { return 'No!' }
    }

By allowing implicit returns we can write the following instead:

    def some_method(value: Integer) {
        (value > 10)
            .if_true  { 'Yes!' }
            .if_false { 'No!' }
    }

## Method Arguments

Method arguments can have an associated type and/or a default value:

    def some_method(number: Integer) {

    }

    def some_method(number = 10) {

    }

Arguments without a default value are required, others are optional. Methods can
also define a rest argument, which is an argument that takes all remaining
arguments passed in when calling the method:

    def some_method(number: Integer, *numbers: Integer) {

    }

Leaving out the types of an argument turns the argument into a dynamically typed
argument. For example, here the `number` argument can contain any type:

    def some_method(number) {

    }

When calling a method its arguments can be specified either in order or using
named arguments:

    def some_method(number) { ... }

    some_method(10)

    some_method(number = 10)

Using named arguments is useful when a method has lots of optional arguments.

## Self

Methods called in a method body without an explicit received are called on the
current instance of a class a method was defined in. As such this:

    def some_method() {
        example()
    }

Is the same as this:

    def some_method() {
        self.example()
    }

Note that in the first example the parenthesis are required as otherwise the
line would be parsed as a local variable reference.

The `self` variable is a special variable always available in a method and can't
be re-assigned. This variable points to the instance a method belongs to.

## Comments

Methods can have comments associated with them, similar to Python docstrings.
Unlike Python this doesn't require any special syntax, instead any comments that
precede a method are directly associated with it.
