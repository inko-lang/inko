---
{
  "title": "Strings"
}
---

The `String` type is a UTF-8 encoded, immutable string. Instances of this type
are created using single or double quotes:

```inko
'hello'
"hello"
```

Strings support [escape sequences and
interpolation](../../references/syntax#strings):

```inko
'hello\tworld'  # => 'hello   world'
'number: ${10}' # => 'number: 10'
```

Single and double quoted strings are the same, i.e. both support escape
sequences and interpolation.

## Ownership and copying

Strings are value types and use atomic reference counting to make copying and
sharing them cheap:

```inko
let a = 'foo'
let b = a
```

Here we can use both `a` and `b` after `b` is defined, because `b` is given a
"copy" of the string, instead of taking over ownership.

## StringBuffer

Because `String` is immutable, operations such as concatenations create new
`String` instances. This can be inefficient when performing operations that
produce many intermediate `String` instances. To work around this, the type
`std.string.StringBuffer` is used:

```inko
import std.string (StringBuffer)

class async Main {
  fn async main {
    let buf = StringBuffer.new

    buf.push('hello ')
    buf.push('world, ')
    buf.push('how are you ')
    buf.push('doing?')
    buf.into_string # => 'hello world, how are you doing?'
  }
}
```

Here we push a bunch of `String` values into the buffer, then concatenate them
together _without_ producing intermediate `String` instances by calling
`StringBuffer.into_string`.

## String slicing

The `String` type offers two ways of slicing up a `String`:

- `String.slice`: slices a `String` into a `ByteArray` using a _byte_ range
- `String.substring`: slices a `String` into another `String` using an _extended
  grapheme cluster_ (i.e. character) range

For example:

```inko
'😊'.slice(start: 0, size: 4)      # => [240, 159, 152, 138]
'😊'.substring(start: 0, chars: 1) # => '😊'
```

Slicing a `String` using `String.slice` is a constant-time operation, while
`String.substring` runs in linear time due to the use of extended grapheme
clusters.

## String iteration

The `String` type offers two ways of iterating over its contents:

- `String.bytes`: returns an iterator over the bytes in the `String`
- `String.chars`: returns an iterator over the extended grapheme clusters in the
  `String`
