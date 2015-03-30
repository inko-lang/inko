# Classes

Aeon is an Object Oriented, class based programming languages. I've been toying
with the idea of making it prototype based, but the end implementation would be
so close to being class based I might as well actually make it class based.
When using prototype based languages a lot of programmers will try to come up
with some way to define a class, which further shows that I might as well just
use a class system.

Having said that, while classes are present and inheritance is supported Aeon
should promote composition using traits and interfaces over creating massive
inheritance chains.

A class definition consists out of a name and an optional parent class. If no
parent class is given the default `Object` class is used. Classes are defined
using the `class` keyword:

    class Example {

    }

To extend another class:

    class Parent {

    }

    class Child extends Parent {

    }

Class bodies can define the instance variables that are available to a class.
All instance variables _must_ be defined initially in the class' body:

    class Example {
        @number = 10
    }

Using `let` is not needed when defining the instance variables in a class' body.
Because the mutability of instance variables is inherited from the mutability of
the class itself one _can not_ declare instance variables as mutable in a class'
body. In other words, this is not valid:

    class Example {
        mut @number = 10
    }

The reason for this is that this could hide certain bugs. For example, a
programmer doesn't mark a class as being mutable and thus expects it to be fully
immutable. However, in secret the class marks an instance variable as immutable,
making the class thread unsafe (unless some kind of synchronization mechanism
were to be used).

An instance of a class can be created using the method `new`, which is available
on all classes:

    let example = Example.new

In case the class mutates its internal state one must declare the entire class
as being mutable:

    class Example {
        @numbers: Array<Integer>

        def add_number(number: Integer) {
            @numbers.push(number)
        }
    }

    let mut example = Example.new

    example.add_number # without "mut" this would be an error

## Built-in Classes

The following built-in classes are available in Aeon:

* Actor
* Array
* Boolean
* Channel
* Class
* File
* Float
* HashMap
* IO
* Integer
* Method
* Object (the root/default class of all others)
* Regexp
* String
* Thread
