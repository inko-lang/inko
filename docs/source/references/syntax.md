---
{
  "title": "Syntax"
}
---

Inko uses a familiar syntax using curly braces, and is easy to understand by
both humans and computers.

## Modules

Each Inko source file is a module and may contain the following:

- Imports
- Constants
- Types
- Methods
- Trait implementations
- Types that are reopened
- Comments

Unlike languages such as Ruby and Python, it's not valid to include expressions
directly in a module, i.e. this is invalid:

```inko
import std.stdio (Stdout)

Stdout.new.print('hello')

type async Main {
  fn async main {}
}
```

## Imports

The `import` statement is used to import a module or symbols from a module. The
syntax is as follows:

```inko
import mod1.mod2        # This imports `mod1.mod2` and exposes it as `mod2`
import mod1.mod2 (A)    # This imports the symbol `A`
import mod1.mod2 (A, B) # This imports the symbols `A` and `B`
import mod1.mod2 (self) # This imports `mod2` from module `mod1`
```

You can also alias symbols when importing them:

```inko
import mod1.mod2 (A as B) # `A` is now exposed as `B`
```

You can also import module methods:

```inko
import std.process (sleep)
```

Imports may specify one or more build tags, resulting in the compiler only
processing the `import` if all the build tags match:

```inko
import foo if linux and amd64
```

The syntax for the tags is `import FOO if TAG1 and TAG2 and ...`.

## External imports

The syntax `import extern "NAME"` is used to link against C libraries. The
string `"NAME"` specifies the library _name_, not the file name or path of the
library. For example, to import libm:

```inko
import extern "m"
```

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

Constants are made public using `let pub`:

```inko
let pub A = 10
```

Strings used as constant values don't support string interpolation.

## Methods

Methods are defined using the `fn` keyword. At the top-level of a module only
instance methods can be defined (i.e. static methods aren't valid directly in a
module). Methods are private by default.

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

# A public mutable async instance method (only available in async types):
fn pub async mut method_name {}

# Method names may end with a ?, this is used for predicate methods:
fn valid? {}

# Method names may also end with a =, this is used for setters:
fn value= {}

# Static and instance methods can use the `inline` keyword to ensure they're
# always inlined:
fn inline foo {}
fn pub inline foo {}
fn pub inline mut foo {}
```

Methods with arguments are defined like so, optional arguments aren't supported:

```inko
fn method_name(arg1: ArgType, arg2: ArgType) {}
```

A return type is specified using `->`:

```inko
fn method_name -> ReturnType {}
```

Type parameters are specified before regular arguments:

```inko
fn method_name[A, B](arg1, A, arg2: B) {}
```

Like with types, you can specify a list of required traits:

```inko
fn method_name[A: ToFoo + ToBar, B](arg1, A, arg2: B) {}
```

The method's body is contained in the curly braces:

```inko
fn method_name {
  [10, 20]
}
```

## External functions

Signatures for C functions are defined using the `fn extern` syntax. These
functions can't define any generic type parameters, can only be defined at the
top-level of a module (i.e. not in a type), and can't specify the `mut`
keyword. For example:

```inko
import extern "m"

fn extern ceil(value: Float64) -> Float64
```

Variadic functions are defined using `...` as the last argument:

```inko
fn extern printf(format: Pointer[UInt8], ...) -> Int32
```

If a body is given, the method is instead _defined_ instead of the compiler
expecting it to be defined elsewhere:

```inko
fn extern example -> Int {
  42
}
```

In this case, variadic arguments are _not_ supported.

## Types

Types are defined using the `type` keyword:

```inko
type Person {}
```

A type can define one or more fields using `let`:

```inko
type Person {
  let @name: String # `@name` is the field name, and `String` its type
  let mut @age: Int
}
```

To make a type public, use `type pub` like so:

```inko
type pub Person {
  let @name: String # `@name` is the field name, and `String` its type
  let @age: Int
}
```

Instances of types are created using the same syntax as method calls:

```inko
Person(name: 'Alice', age: 42)
```

Positional arguments are also supported:

```inko
Person('Alice', 42)
```

If no fields are given, the parentheses are required:

```inko
type Example {}

Example()
```

### Enums

Enums are defined as follows:

```inko
type pub enum Result {}
```

Enum types allow defining of constructors using the `case` keyword:

```inko
type enum Result {
  case Ok
  case Error
}
```

Enum types can't define regular fields.

### Stack allocated types

Stack allocated types are defined using the `inline` and `copy` keywords:

```inko
type pub copy Example {}
type pub inline Example {}
```

These keywords can be combined with the `enum` keyword:

```inko
type copy enum Example {
  case A
  case B
}

type inline enum Example {
  case A
  case B
}
```

### Generic types

Generic types are defined like so:

```inko
type enum Result[T, E] {
  case Ok(T)
  case Error(E)
}
```

Here `T` and `E` are type parameters. Type parameters can also list one or more
traits that must be implemented before a type can be assigned to the parameter:

```inko
type enum Result[T, E: ToString + ToFoo + ToBar] {
  case Ok(T)
  case Error(E)
}
```

Type parameters can also specify the `mut` requirement, restricting the types to
those that allow mutations:

```inko
type MyBox[T: mut] {}
```

### Processes

Processes are defined as async types like so:

```inko
type async Counter {

}

type pub async Counter {

}
```

### C structures

C structures are defined using `type extern`. When used, the type can't define
any methods or use generic type parameters:

```inko
type extern Timespec {
  let @tv_sec: Int64
  let @tv_nsec: Int64
}
```

### Methods

Types can define static methods, instance methods, and async methods (in case
the type is an async type):

```inko
type Person {
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

type async Counter {
  fn async increment {
    # ...
  }
}
```

### Reopening types

A type can be reopened using the `impl` keyword like so:

```inko
impl String {}
```

Within the body, only methods are allowed; fields can only be defined when the
type is defined for the first time.

## Traits

Traits are defined using the `trait` keyword, and like types default to being
private.

Traits are defined like so:

```inko
trait ToString {}

trait pub ToString {}
```

### Required traits

Traits can specify a list of other traits that must be implemented before the
trait itself can be implemented:

```inko
trait Debug: ToString + ToFoo {}
```

### Methods

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

Traits can't specify static or async methods.

Traits can also define type parameters:

```inko
trait ToArray[T] {}
```

And like types and methods, these can define required traits:

```inko
trait ToArray[T: ToFoo + ToBar] {}
```

Type parameters can also specify the `mut` requirement, restricting the types to
those that allow mutations:

```inko
trait ToArray[T: ToFoo + mut] {}
```

### Implementing traits

Traits are implemented using the `impl` keyword:

```inko
impl ToString for String {}
```

The syntax is `impl TraitName for TypeName { body }`. Within the body only
instance methods are allowed.

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

## Identifiers

Identifiers are referred to by just using their name:

```inko
this_is_an_identifier
```

Identifiers can contain the following characters: `a-z`, `A-Z`, `0-9`, `_`, `$`,
and may end with a `?`.

::: warn
While identifiers can contain dollar signs (e.g. `foo$bar`), this is only
supported to allow use of C functions using a `$` in the name, such as
`readdir$INODE64` on macOS. **Do not** use dollar signs in regular Inko method
or variable names.
:::

The `self` keyword is used to refer to the receiver of a method. This keyword is
available in all methods, including module and static methods.

## Field references

Fields are referred to using `@NAME` where `NAME` is the name of the field:

```inko
@address
```

## Constant references

Constants are referred to as follows:

```inko
let NUMBER = 42

fn example {
  NUMBER
}
```

It's also possible to refer to a constant in a module like so:

```inko
import foo

fn example {
  foo.SOME_CONSTANT
}
```

## Scopes

Scopes are created using curly braces:

```inko
'foo'

{       # <- This is the start of a new scope
  'bar'
}       # <- The scope ends here
```

## Strings

Inko has two types of strings: single quoted and double quoted strings. Both
types of strings are the same semantically. Strings support the following escape
sequences:

- `\"`
- `\'`
- `\0`
- `\\`
- `\e`
- `\n`
- `\r`
- `\t`
- `\#`

For example:

```inko
'hello\nbar'
"hello\nbar"
```

Strings can span multiple lines:

```inko
"this string spans
multiple
lines"
```

Strings support Unicode escape sequences using the syntax `\u{XXXXX}`:

```inko
'foo\u{AC}bar'
```

Strings support string interpolation:

```inko
let name = 'Alice'

"hello ${name}" # => 'hello Alice'
'hello ${name}' # => 'hello Alice'
```

Here `${` marks the start of the embedded expression, and `}` the end. To use a
literal `${`, escape the `$` sign:

```inko
let name = 'Alice'

'hello \${name}' # => 'hello \${name}'
```

## Integers

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

## Floats

Floats are created using the usual floating point syntax:

```inko
10.5
-10.5
10e+2
10E+2
```

## Arrays

Arrays are created using flat brackets:

```inko
[]
[10]
[10, 20]
```

## Booleans

Booleans are created using `true` and `false`. A boolean is negated using the
`!` prefix operator, i.e. `!expression`. For example:

```inko
if !volume_is_too_loud {
  turn_volume_to(11)
}
```

When used with the logical operators `and` and `or`, the `!` operator has a
higher precedence. This means that this:

```inko
!a and b
```

Is parsed as:

```inko
(!a) and b
```

Instead of:

```inko
!(a and b)
```

## Nil

The `nil` keyword is used to create an instance of `Nil`.

## Conditionals

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

## Loops

`for` loops use the following syntax:

```inko
for PATTERN in ITERATOR {
  ...
}
```

For example:

```inko
for (key, value) in [('name', 'Alice'), ('city', 'Example Town')] {
  ...
}
```

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

## Pattern matching

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
- String literals: `case 'foo' -> BODY`
- Constants: `case FOO -> BODY`
- Bindings: `case v -> BODY`, `case mut v -> BODY` (allows reassigning of `v`)
- Wildcards: `case _ -> BODY`
- Enum constructors: `case Some(v) -> BODY`
- Types: `case { @name = name } -> BODY`
- Tuples: `case (a, b, c) -> BODY`
- Arrays: `case [a, b, c] -> BODY`
- OR patterns: `case 10 or 20 -> BODY`

Guards are also supported:

```inko
match foo {
  case Some(num) if num > 10 -> foo
  case _ -> bar
}
```

String literals used as patterns _don't_ support string interpolation.

## Closures

Closures are created using the `fn` keyword. Unlike methods, the argument types
are optional.

```inko
fn {}
fn (a, b) {}
fn (a: Int, b: Int) {}
fn (a, b) -> ReturnType {}
```

## Tuples and grouping

Expressions are grouped using parentheses:

```inko
(10 + 5) / 2 # => 7
```

Tuples are also created using parentheses, but must contain at least a single
comma:

```inko
(10)     # => Int
(10,)    # => Tuple1[Int]
(10, 20) # => Tuple2[Int, Int]
```

## Borrows

Borrows are created using `ref` and `mut`:

```inko
ref foo
mut bar
```

## Recover expressions

Recovering is done using the `recover` keyword:

```inko
recover foo
recover { foo }
```

## Returning values

The last value in a body is its return value. Explicitly returning values is
done using `return`:

```inko
return 42
```

Throwing values is done using `throw`:

```inko
throw 42
```

This is syntax sugar for the following:

```inko
return Result.Error(42)
```

## Error handling

`try` is available as syntax sugar for a `match`, and supports values of type
`Result` and `Option`.

`try option` (where `option` is an `Option`) is syntax sugar for the following:

```inko
match option {
  case Some(val) -> val
  case None -> return Option.None
}
```

`try result` (where `result` is a `Result`) is syntax sugar for the following:

```inko
match result {
  case Ok(val) -> val
  case Error(error) -> return Result.Error(error)
}
```

## Defining variables

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

`let` supports pattern matching using the following syntax:

```inko
let PATTERN = EXPRESSION
let PATTERN = EXPRESSION else EXPRESSION
```

For example:

```inko
let Some((a, b)) = example else return
```

## Assigning variables

Variables are assigned a new value using the syntax `VAR = VAL`. Inko also
supports swapping of values using the syntax `VAR := VAL`:

```inko
let mut a = 42

a := 50 # => 42
a       # => 50
```

## Method calls

Methods without arguments can be called without parentheses. If arguments _are_
given, parentheses are required:

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

## Binary expressions

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
, `&` , `^` , `==` , `!=`, `>>>`

Inko also supports two logical operators: `and` and `or`. These operators have a
higher precedence than the regular binary operators. This means
`1 + 2 and 3 + 4` is parsed as `(1 + 2) and (3 + 4)`. `and` and `or` have the
same precedence as each other.

## Indexing

Inko doesn't have dedicated indexing syntax such as `array[index]` or
`array[index] = value`, instead you'd use the methods `get`, `get_mut`, and
`set` like so:

```inko
let numbers = [10, 20]

numbers.get(1) # => Result.Ok(20)
numbers.set(1, 30)
numbers.get(1) # => Result.Ok(30)
```

## Type casts

Type casting is done using `as` like so:

```inko
expression as TypeName
```

The `as` keyword has the same precedence as binary operators. This means that
this:

```inko
10 + 5 as ToString
```

Is parsed as this:

```inko
(10 + 5) as ToString
```

And this:

```inko
foo as Int + 5 as Foo
```

Is parsed as this:

```inko
(foo as Int + 5) as Foo
```
