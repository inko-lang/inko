# Methods

Methods are routines defined on an object be it a class or an instance. A method
has a name, a list of arguments (optional) and an optional return type. If no
return type is specified it's assumed the method returns void (= nothing).

A method is defined using the `def` keyword followed by a name and the argument
list:

    def some_method() {

    }

The parenthesis are required, even when the method has no arguments. If a method
has a return value you can specify the return type in the method definition
using the `->` symbol:

    def some_method() -> Integer {
        return 10
    }

It's a compiler error to define a return type but not return anything, just as
the reverse is also an error. Returning values requires the use of the `return`
keyword, there's no implicit return (e.g. like Ruby).

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

It's a syntax error for regular arguments to be defined after a rest argument.
The full syntax of a method definition is as following:

    method
      = 'def' identifier '(' arg_list ')' ('->' constant)? '{' expressions '}'
      ;

    arg_list
      = argument (',' argument)*
      | _
      ;

    argument
      = '*'? identifier (':' constant)? ('=' expression)?
      ;

Methods can have comments associated with them, similar to Python docstrings.
Unlike Python this doesn't require any special syntax, instead any comments that
precede a method are directly associated with it.
