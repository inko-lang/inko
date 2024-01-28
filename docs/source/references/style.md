---
{
  "title": "Style guide"
}
---

This guide documents the best practises to follow when writing Inko source code,
such as what indentation method to use, and when to use keyword arguments.

## Encoding

Inko source files must be encoded in UTF-8. The compiler does not support
reading source files using a different encoding.

## Line endings

Unix (`\n`) line endings must be used at all times. Windows newlines (`\r`) are
not supported.

## Line length

Lines should be hard wrapped at 80 characters per line. It's OK if a line is a
few characters longer, but only if wrapping the line makes it less readable. For
example, if a line's length is dominated by a string, then it's OK to keep that
on a single line.

## Indentation

Inko source code should be indented using 2 spaces per indentation level, not
tabs. Different programs use different widths for tabs (sometimes with no way of
changing this), potentially making source code harder to read. By using spaces
_only_ we prevent the accidental mixing of tabs and spaces, and ensure Inko code
always looks consistent.

Inko relies heavily on blocks, which can lead to lots of indentation levels.
Using 4 spaces per indentation level would consume too much horizontal space, so
we use 2 spaces instead.

Place opening curly braces on the same line as the expression that precedes
them:

```inko
if foo {

}
```

This applies to all expressions, such as `if`, `try`, `while`, etc.

## Naming

Types use PascalCase, such as `ByteArray` and `String`:

```inko
class AddressFormatter {}
```

Methods, local variables, instance attributes, and arguments all use snake\_case
for naming, such as `to_string` and `write_bytes`:

### Let constants

Constants defined using `let` use SCREAMING_SNAKE_CASE, such as `DAY_OF_WEEK` or
`NUMBER`:

```inko
let FIRST_DAY_OF_WEEK = 'Monday'
```

### Argument names

Arguments should use human readable names, such as `address`. Avoid the use of
abbreviations such as `num` instead of `number`. Every argument is a keyword
argument, and the use of abbreviations can make it harder for a reader to figure
out what the meaning of an argument is.

### Predicates

When defining a method that returns a `Boolean`, end the method name with a `?`:

```inko
fn allowed? -> Boolean {
  # ...
}
```

This removes the need for prefixing your method names with `is_`, such as
`is_allowed`.

### Traits

Traits should be a given a clear name such as `ToArray` or `Index`. Don't use
the pattern of `[verb]-ble` such as `Enumerable` or `Iterable`.

### Conversion methods

Methods that convert one type into another should be prefixed with `to_`,
followed by a short name of the type. Examples include `to_array`, `to_string`,
`to_coordinates`, etc. If a value is _moved into_ another type, use `into_` as
the prefix instead.

## Defining methods

If a method does not take any arguments, leave out the parentheses:

```inko
fn example {}
```

If a method definition does not fit on a single line, place every argument on a
separate line, followed by a comma. The last argument should also be followed by
a comma:

```inko
fn example(
  foo: A,
  bar: B,
) {

}
```

If a return type is given, place it on the same line as the closing parenthesis,
if possible:

```inko
fn example(
  foo: A,
  bar: B,
) -> ReturnType {

}
```

Type arguments should be placed on the same line as the method name.

```inko
fn example[A, B](foo: A, bar: B) {

}
```

If this doesn't fit, the same rules apply as used for regular arguments:

```inko
fn example[
  A,
  B,
](foo: A, bar: B) {

}
```

Again, such code is best avoided, as it can be a bit hard to read.

## Parentheses

Inko allows you to leave out the parentheses when a method doesn't take any
arguments, or when only a single argument is provided and it's a closure.

When calling a method without arguments, leave out the parentheses:

```inko
[10, 20, 30].pop
```

If the only argument is a closure, leave out the parentheses:

```inko
[10, 20, 30].each fn (number) {
  # ...
}
```

If there are multiple arguments, and the last one is a closure, use parentheses
and place the closure outside them:

```inko
t.test('This is a test') fn (t) {

}
```

When the number of arguments don't fit on a single line, place each argument on
its own line like so:

```inko
some_object.some_message_name(
  10,
  20,
  30,
)
```

When spreading arguments across multiple lines, end the last argument with a
comma:

```inko
some_object.some_message_name(
  10,
  20,
  30,
)
```

By using a trailing comma, adding a new argument is easier as you
don't need to first add a comma to the current last argument, before adding a
new argument. When removing lines this also leads to smaller diffs.

## Message chains

When chaining multiple messages together that don't fit on a single line,
place every message on a separate line:

```inko
foo
  .bar
  .baz
```

## Named arguments

The use of named arguments is recommended whenever this enhances the readability
of code. Take the following code for example:

```inko
'hello'.slice(0, 2)
```

Looking at this code, it's not clear what the values `0` and `2` are used for.
Using keyword arguments this becomes obvious:

```inko
'hello'.slice(start: 0, chars: 2)
```

When passing variables with the same name as the arguments, you can leave out
named arguments:

```inko
# This is redundant.
'hello'.slice(start: start, chars: chars)

# This is fine.
'hello'.slice(start, chars)
```

## Comments

Comments should be used to describe intent, provide examples, and explain
certain decisions that might not be obvious to the reader. Comments should _not_
be used to explain what the code does in its literal sense.

When documenting a type, constant, or method, the first line of the comment
should be a short summary. This summary should be about one sentence long and
describe the purpose of the item. For example:

```inko
# A Person can be used for storing details of a single person, such as their
# name and address.
class Person {

}
```

When documenting a module, start the comment on the first line of the module,
before any imports:

```inko
# The documentation of the module goes here.
import std.stdio.STDOUT
```

## Imports

Imports should be placed at the top of a module, in alphabetical order:

```inko
import std.fs.file
import std.stdio.stdout
```

The symbols imported from a module should also be listed in alphabetical order.
If `self` is imported, it should come first:

```inko
import std.fs.file.(self, ReadOnlyFile)
```

## Class literals

Constructing an instance of a class uses the following syntax:

```inko
TypeName { @attribute = 'value' }
```

When constructing an instance, place the expression on a single line if it fits:

```inko
Person { @name = 'Alice', @age = 32 }
```

If it doesn't fit, put every attribute assignment on a separate line:

```inko
Person {
  @name = 'Alice',
  @age = 32,
}
```

If a class doesn't define any attributes, construct your instance as follows:

```inko
Person {}
```
