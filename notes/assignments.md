# Variable Assignments

Aeon has 3 types of variable assignments:

1. Local variable assignments
2. Instance variable assignments
3. Dynamic variable assignments

## Local Variable Assignments

To distinguish local variables from method calls (due to `self` being implicit),
Aeon requires one to use the `let` keyword in order to declare a local variable.
For example:

    let number = 10

By default variables (and all other objects) are immutable. This means that the
variable can not be re-assigned, nor can the stored value be mutated. To make a
variable mutable one should use the `mut` keyword:

    let mut number = 10

    number = 20

It's a compiler error to re-assign an immutable variable. Marking a variable as
mutable also allows one to modify the contents:

    let mut numbers = [10, 20, 30]

    numbers.push(40)

If `mut` were omitted in the above example this would lead to a compiler error.

It's a compiler error to re-assign a variable to  different type, even when the
variable is mutable:

    let mut number = 10

    number = 'Alice' # compiler error

## Instance Variable Assignments

Instance variables are variables that are available to an instance of a class.
These are the same as "class instance variables" in Smalltalk and "instance
variables" in Ruby.

Because instance variables are defined in a class' body (opposed to in a method)
there's no need to use `let` to assign a value to the variable. For example:

    class Person {
        @name: String

        def construct() {
            @name = 'something'
        }
    }

Unlike regular variables an instance variable must be prefixed by a `@` to
distinguish it from a local variable.

Unlike local variables you can not use the `mut` keyword for instance variables
as mutability of these variables is inherited from the instance of a class. That
is, instance variables are only mutable if the instance itself has been declared
mutable.

## Dynamic Variable Assignments

Sometimes you just don't want to bother with statically typed variables. In such
a case you mark a variable as being of a dynamic type.

Marking a variable as dynamic means you _can_ re-assign the variable and you
_can_ re-assign it a different type. However, you can not modify the value
itself unless you also mark the variable as being mutable.

Dynamic variables are declared by simply omitting the `let` keyword:

    number = 10
    number = 'Alice' # This works fine

This however doesn't mark the value the variable holds as being mutable, you
still have to use the `mut` keyword for this:

    mut numbers = [10, 20, 30]

    numbers.push(40) # ok!

Instance variables can be turned into dynamically typed variables by simply
omitting the type signature:

    class Example {
        @number
    }
