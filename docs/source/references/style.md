---
{
  "title": "Style guide"
}
---

This document provides a high-level overview of Inko's style guide, covering
topics such as when to wrap lines, what indentation to use, and more. This guide
is _not_ a low-level specification of how to precisely format every syntax
construct, instead the `inko fmt` command acts as the specification.

## Using inko fmt

The `inko fmt` command formats source files according to the Inko style guide.
You can use it to format specific files as follows:

```bash
inko fmt foo.inko bar.inko
```

You can also format the entire project like so:

```bash
inko fmt
```

Source code can also be passed through STDIN, in which case you need to use
`inko fmt -`:

```bash
echo 'fn example() {}' | inko fmt -
```

In this case the output is written to STDOUT.

You can check for any files that need formatting using `inko fmt --check`. This
won't update any files, instead it prints any files that need to be formatted
and exits with exit code 1 (or 0 if all files are formatted correctly).

## Encoding

Inko source files must be encoded in UTF-8. The compiler does not support
reading source files using a different encoding.

## Line endings

Unix (`\n`) line endings must be used at all times. Windows newlines (`\r`) are
not supported.

## Line length

Lines should be hard wrapped at 80 characters per line. If a string literal
doesn't fit on a single line, it's fine to leave it as-is.

## Indentation

Inko source code should be indented using 2 spaces per indentation level, not
tabs. Different programs use different widths for tabs (sometimes with no way of
changing this), potentially making source code harder to read. By using spaces
_only_ we prevent the accidental mixing of tabs and spaces, and ensure Inko code
always looks consistent.

## Naming

Types use PascalCase, such as `ByteArray` and `String`:

```inko
class AddressFormatter {}
```

Methods, local variables, instance attributes, and arguments all use snake\_case
for naming, such as `to_string` and `write_bytes`:

### Constants

Constants defined using `let` use `SCREAMING_SNAKE_CASE`, such as `DAY_OF_WEEK`
or `NUMBER`:

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

## Arguments

When calling a method without arguments, leave out the parentheses:

```inko
[10, 20, 30].pop
```

If the last argument of a method call is a closure, format it like so:

```inko
[10, 20, 30].each(fn (number) {
  # ...
})
```

When the number of arguments don't fit on a single line, place each argument on
its own line and use a trailing comma for the last argument:

```inko
some_object.some_message_name(
  10,
  20,
  30,
)
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
import std.stdio (STDOUT)
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
import std.fs.file (self, ReadOnlyFile)
```
