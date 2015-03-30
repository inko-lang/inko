# Type System

Aeon is gradually typed, meaning that some parts can be statically typed while
other parts can be dynamically typed. This allows a developer to choose what
style is the best for the task at hand.

By default Aeon is statically typed, so a developer must explicitly mark
something as being dynamic. Data being statically typed not only restricts the
types, but also prevents re-assignment of variables and attributes.

Types are made up out of the following entities:

* Classes
* Traits
* Interfaces

In other words, a class is a type but a type is not limited to only being a
class.

When code is statically typed the compiler can perform extra checks. For
example, calls to a statically typed method can be verified during compilation.
This makes it easier to catch errors such as not specifying all required
arguments, incompatible types, etc.

Ideally the compiler would also be able to spot method calls to undefined
methods. This would be quite difficult, if not impossible to implement if Aeon
were to support `method_missing` (from Ruby) and/or runtime method definitions
(similar to Ruby's `Object#define_method`).

I envision two options:

1. Simply don't check if a method is defined during compilation, handle it
   during runtime instead.
2. When a class is marked as `final` (keyword up for debate) this means it can't
   be modified during runtime. This in turn would allow the compiler to validate
   all method calls invoked on an instance of such a class.

The problem with option 2 is that this requires extra work from the programmer.
On top of that it would require tagging all core classes as `final` in order to
ensure that the compiler can validate method calls on these classes.

Perhaps as a start its best to only check for the existence of a method during
runtime, this would also make the compiler a bit less complicated.

## Type Syntax

In most cases the Aeon compiler should be able to infer the type of a variable
based on the value assigned to it. In some cases this is not possible, one of
those examples would be an instance variable without a default value.

The syntax used for annotating types is as following:

    type = identifier ':' constant

For example:

    number: Integer

This can be combined with a default value, although this is not required as the
type can be inferred based on the value:

    let number: Integer = 10

The syntax is the same for arguments:

    def some_method(number: Integer) {

    }

And for instance variables:

    @number: Integer
