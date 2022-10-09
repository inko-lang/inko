# Syntax

Inko uses a familiar syntax using curly braces, and is easy to understand by
both humans and computers.

If you're new to Inko you don't have to read this entire guide right away,
instead we recommend using it more as a reference as you make your way through
the rest of the documentation.

## Modules

Each Inko source file is a module and may contain the following:

- Imports
- Constants
- Classes
- Methods
- Trait implementations
- Classes that are reopened
- Comments

Unlike languages such as Ruby and Python, it's not valid to include expressions
directly in a module.

## Comments

Comments start with a `#` and continue until the end of the line:

```inko
# This is a comment that spans a single line
```

Multiple uninterrupted comment lines are treated as a single comment:

```inko
# This is
# a single
# comment.

# But this is a separate comment due to the empty line above.
```

## Imports

The `import` statement is used to import a module or symbols from a module. The
syntax is as follows:

```inko
import mod1::mod2         # This imports `mod1::mod2` and exposes it as `mod2`
import mod1::mod2::A      # This imports the symbol `A`
import mod1::mod2::(A, B) # This imports the symbol `A` and `B`
import mod1::mod2::(self) # This imports `mod2` from module `mod1`
```

You can also alias symbols when importing them:

```inko
import mod1::mod2::(A as B) # `A` is now exposed as `B`
```

You can also import module methods:

```inko
import std::process::(sleep)
```

This always requires the use of parentheses in the symbol list, otherwise the
compiler would think you're trying to import the module `std::process::sleep`.

## Constants

Constants are defined using `let` at the module top-level. Constants are limited
to integers, floats, strings, arrays of constants, and binary expressions of
constants:

```inko
let A = 10
let B = 10.5
let C = 'foo'
let D = A + 5
let E = [A, 10]
let F = 10 + A
```

## Classes

Classes are defined using the `class` keyword:

```inko
class Person {}
```

A class can define one or more fields using `let`:

```inko
class Person {
  let @name: String # `@name` is the field name, and `String` its type
  let @age: Int
}
```

Classes default to being private to the module they are defined in. To make a
class public, use `class pub` like so:

```inko
class pub Person {
  let @name: String # `@name` is the field name, and `String` its type
  let @age: Int
}
```

Inko also supports algebraic data types (also known as enums), which are defined
like so:

```inko
class enum Result {}

class pub enum Result {}
```

Enum classes allow defining of variants using the `case` keyword:

```inko
class enum Result {
  case Ok
  case Error
}
```

Enum classes can't define regular fields.

Generic classes are defined like so:

```inko
class enum Result[T, E] {
  case Ok(T)
  case Error(E)
}
```

Here `T` and `E` are type parameters. Type parameters can also list one or more
traits that must be implemented before a type can be assigned to the parameter:

```inko
class enum Result[T, E: ToString + ToFoo + ToBar] {
  case Ok(T)
  case Error(E)
}
```

Processes are defined as async classes like so:

```inko
class async Counter {

}

class pub async Counter {

}
```

Classes can define static methods, instance methods, and async methods (in case
the class is an async class):

```inko
class Person {
  let @name: String

  # This is a static method, available as `Person.new`.
  fn static new(name: String) -> Person {
    # ...
  }

  # This is an instance method, which is only available to instances of
  # `Person`.
  fn name -> String {
    # ...
  }
}

class async Counter {
  # An async instance method (more on this in a separate chapter).
  fn async increment {
    # ...
  }
}
```

## Methods

Methods are defined using the `fn` keyword. At the top-level of a module only
instance methods can be defined (i.e. static methods aren't valid directly in a
module). Methods are private to their modules by default.

Methods use the following syntax:

```inko
# A private immutable instance method:
fn method_name {}

# A public immutable instance method:
fn pub method_name {}

# A private mutable instance method:
fn mut method_name {}

# A private moving method:
fn move method_name {}

# A public mutable instance method:
fn pub mut method_name {}

# A public mutable async instance method (only available in async classes):
fn pub async mut method_name {}

# Method names may end with a ?, this is used for predicate methods:
fn valid? {}

# Method names may also end with a =, this is used for setters:
fn value= {}
```

Methods with arguments are defined like so, optional arguments aren't supported:

```inko
fn method_name(arg1: ArgType, arg2: ArgType) {}
```

A throw type is specified using `!!`:

```inko
fn method_name !! ThrowType {}
```

A return type is specified using `->`:

```inko
fn method_name -> ReturnType {}
```

If you need both, the throw type must come first:

```inko
fn method_name !! ThrowType -> ReturnType {}
```

Type parameters are specified before regular arguments:

```inko
fn method_name[A, B](arg1, A, arg2: B) {}
```

Like classes, you can specify a list of required traits:

```inko
fn method_name[A: ToFoo + ToBar, B](arg1, A, arg2: B) {}
```

The method's body is contained in the curly braces:

```inko
fn method_name {
  [10, 20]
}
```

## Taits

Traits are defined using the `trait` keyword, and like classes default to being
private to their modules.

Traits are defined like so:

```inko
trait ToString {}

trait pub ToString {}
```

Traits can specify a list of other traits that must be implemented before the
trait itself can be implemented:

```inko
trait Debug: ToString + ToFoo {}
```

Traits can define both default and required methods:

```inko
trait ToString {
  # A required instance method:
  fn to_string -> String

  # A default instance method:
  fn to_foo -> String {
    'test'
  }
}
```

Traits aren't allowed to define static methods.

Traits can also define type parameters:

```inko
trait ToArray[T] {}
```

And like classes and methods, these can define required traits:

```inko
trait ToArray[T: ToFoo + ToBar] {}
```

## Implementing traits

Traits are implemented using the `impl` keyword:

```inko
impl ToString for String {}
```

The syntax is `impl TraitName for ClassName { body }`. Within the body only
instance methods are allowed.

## Reopening classes

A class can be reopened using the `impl` keyword like so:

```inko
impl String {}
```

Within the body, only methods are allowed; fields can only be defined when the
class is defined for the first time.

## Expressions

Each method's body can contain zero or more expressions.

### Identifiers

Identifiers are referred to by just using their name:

```inko
this_is_an_identifier
```

Identifiers are limited to ASCII, though they may end in a `?` (used for
predicate methods):

The `self` keyword is used to refer to the receiver of a method. This keyword is
available in all methods, including module and static methods.

### Field references

Fields are referred to using `@NAME` where `NAME` is the name of the field:

```inko
@address
```

### Constant references

Constants can be referred to by just using their name:

```inko
let NUMBER = 42

fn example {
  NUMBER
}
```

### Scopes

Scopes are created using curly braces:

```inko
'foo'

{       # <- This is the start of a new scope
  'bar'
}       # <- The scope ends here
```

### Strings

Inko has two types of strings: single quoted strings and double quoted strings.
Double quoted strings allow the use of escape sequences such as `\n` and support
string interpolation:

```inko
'foo\nbar'       # => "foo\\nbar" (as in a literal \n, not a newline)
"foo\nbar"       # => "foo\nbar"
"foo{10 + 5}bar" # => "foo15bar"
```

Strings can span multiple lines:

```inko
"this string spans
multiple
lines"
```

If a string spans multiple lines and a line ends with a `\`, the newline and any
whitespace that follows is ignored:

```inko
"foo \
bar \
baz" # => "foo bar baz"
```

Double quoted strings support Unicode escape sequences using the syntax
`\u{XXXXX}`, such as this:

```inko
"foo\u{AC}bar"
```

### Integers

The syntax for integers is as follows:

```inko
10
0x123
```

Underscores in integer literals are ignored, and are useful to make large
numbers more readable:

```inko
123_456_789
```

### Floats

Floats are created using the usual floating point syntax:

```inko
10.5
-10.5
10e+2
10E+2
```

### Arrays

Arrays are created using flat brackets:

```inko
[]
[10]
[10, 20]
```

### Booleans

Booleans are created using `true` and `false`.

### Nil

The `nil` keyword is used to create an instance of `Nil`.

### Class literals

Class literals start with the name of the class followed by curly braces. Within
the curly braces, zero or more fields are assigned a value:

```inko
class Person {
  let @name: String
}

Person { @name = 'Alice' }
```

### Conditionals

`if` expressions use the `if` keyword:

```inko
if foo {
  one
} else if bar {
  two
} else {
  three
}
```

### Loops

Infinite loops are created using the `loop` keyword:

```inko
loop {
  # ...
}
```

Conditional loops are created using the `while` keyword:

```inko
while condition {
  # ...
}
```

`break` and `next` can be used to break out of a loop or jump to the next
iteration respectively. Breaking a loop with a value (e.g. `break 42`) isn't
supported.

### Pattern matching

Pattern matching is performed using the `match` keyword:

```inko
match value {
  case PAT -> BODY
}
```

Each case starts with the `case` keyword, specifies one or more patterns, an
optional guard, and a body to run if the pattern matches.

The following patterns are supported:

- Integer literals: `case 10 -> BODY`
- Float literals: `case 10.5 -> BODY`
- String literals: `case 'foo' -> BODY`
- Constants: `case FOO -> BODY`
- Bindings: `case v -> BODY`
- Wildcards: `case _ -> BODY`
- Variants: `case Some(v) -> BODY`
- Class literals: `case { @name = name } -> BODY`
- Tuples: `case (a, b, c) -> BODY`
- OR patterns: `case 10 or 20 -> BODY`

Guards are also supported:

```inko
match foo {
  case Some(num) if num > 10 -> foo
  case _ -> bar
}
```

### Closures

Closures are created using the `fn` keyword:

```inko
fn {}
fn (a, b) {}
fn (a: Int, b: Int) {}
fn (a, b) !! ErrorType -> ReturnType {}
```

Unlike methods, the argument types are optional.

### Tuples and grouping

Expressions are grouped using parentheses:

```inko
(10 + 5) / 2 # => 7
```

Tuples are also created using parentheses, but must contain at least a single
comma:

```inko
(10,)    # => Tuple1[Int]
(10, 20) # => Tuple2[Int, Int]
```

### References

References are created using `ref` and `mut`:

```inko
ref foo
mut bar
```

### Recover expressions

Recovering is done using the `recover` keyword:

```inko
recover foo
recover { foo }
```

### Return and throw

The last value in a body is its return value. Explicitly returning values is
done using `return`:

```inko
return 42
```

Throwing values is done using `throw`:

```inko
throw
```

### Error handling

`try` and `try!` are used for error handling:

```inko
try foo
try foo else bar
try foo else (error) bar
try { foo } else bar
try { foo } else { bar }
try { foo } else (error) { bar }

try! foo
try! { foo }
```

### Defining variables

Variables are defined using `let`:

```inko
let number = 42
let number: Int = 42
let _ = 42
```

By default a variable can't be assigned a new value. To allow this, use `let
mut`:

```inko
let mut number = 42

number = 50
```

Pattern matching in a `let` isn't supported.

### Assigning variables

Variables are assigned a new value using the syntax `VAR = VAL`. Inko also
supports swapping of values using the syntax `VAR := VAL`.

### Method calls

Methods without arguments can be called without parentheses. If arguments _are_
defined, parentheses are required:

```inko
self.foo    # calls foo() on self
self.foo()  # Same thing
foo(10, 20)
self.foo(10, 20)
```

Named arguments are also supported:

```inko
foo(name: 10, bar: 20)
```

When using named arguments, they must come _after_ positional arguments:

```inko
foo(10, bar: 20)
```

If the last argument is a closure, it can be specified after the parentheses:

```inko
foo(10) fn { bar } # Same as `foo(10, fn { bar })`
```

### Binary expressions

Binary operator expressions use the following syntax:

```
left OP right
```

For example:

```inko
10 + 5
```

Operators are left-associative. This means `5 + 10 * 2` evaluates to `30`, _not_
`25`. The following binary operators are supported:

`+` , `-` , `/` , `*` , `**` , `%` , `<` , `>` , `<=` , `>=` , `<<` , `>>` , `|`
, `&` , `^` , `==` , `!=`

Inko also supports two logical operators: `and` and `or`. These operators have a
higher precedence than the regular binary operators. This means
`1 + 2 and 3 + 4` is parsed as `(1 + 2) and (3 + 4)`. `and` and `or` have the
same precedence as each other.

### Async method calls

Async calls start with the `async` keyword, and only support method calls
with an explicit receiver:

```inko
async foo.bar
async { foo.bar }
```

### Indexing

Indexing is done using `[]` and `[]=` like so:

```inko
numbers[0]
numbers[0] = 42
```

### Type casts

Type casting is done using `as` like so:

```inko
expression as TypeName
```

The `as` keyword has a higher precedence than the binary and logical operators,
meaning that this:

```inko
10 + 5 as ToString
```

Is parsed as this:

```inko
(10 + 5) as ToString
```
