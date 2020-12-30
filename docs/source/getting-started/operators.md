# Operators

Inko supports a variety of different operators, such as `-` and `%`. These
operators are infix operators, meaning you place the operators between the
operands. For example, to subtract `b` from `a` you'd write `a - b`. We call
such expressions "binary expressions".

Unlike other languages, operators aren't limited to just numerical types.
In fact, they are messages that any object can respond to; provided they
implement the right trait(s).

## Available operators

The following operators are available:

| Operator | Example  | Trait to implement
|:---------|:---------|:-------------------
| `+`      | `a + b`  | `Add`
| `/`      | `a / b`  | `Divide`
| `*`      | `a * b`  | `Multiply`
| `-`      | `a - b`  | `Subtract`
| `%`      | `a % b`  | `Modulo`
| `<`      | `a < b`  | `Smaller`
| `>`      | `a > b`  | `Greater`
| `==`     | `a == b` | `Equal`
| `>=`     | `a >= b` | `GreaterOrEqual`
| `<=`     | `a <= b` | `SmallerOrEqual`
| `&`      | `a & b`  | `BitwiseAnd`
| `|`      | `a | b`  | `BitwiseOr`
| `^`      | `a ^ b`  | `BitwiseXor`
| `<<`     | `a << b` | `ShiftLeft`
| `>>`     | `a >> b` | `ShiftRight`
| `=~`     | `a =~ b` | `Match`

You can find these traits in the `std::operators` module, and must import them
if you want to implement them for your type(s).

Inko does not support prefix operators such as `!foo`.

## Operator precedence

It's common for languages to give different operators a different precedence.
For example, in Ruby `1 + 2 * 4` is parsed as `1 + (2 * 4)`, producing `9` as
the result. In Inko, the associativity of operators is left-associative. This
means that `1 + 2 * 4` is parsed as `(1 + 2) * 4`, producing `12` as the result.

Coming from other languages this may take some getting used to, and may even
seen as a bug. But the choice is deliberate: it's easier to remember operator
precedence if it's the same for all operators, instead of some operators having
a different precedence.

If you want to force a different precedence, you want wrap an expression in
parentheses. For example, if we want `1 + 2 * 4` to be parsed as is done in
Ruby you' write:

```inko
1 + (2 * 4)
```

## Eagerness

All operates use eager evaluation, and there is no way to evaluate operands in a
lazy fashion.

## Boolean operators

Most languages provide a `&&` (boolean AND) and `||` (boolean OR) operator.
These operators typically use lazy evaluation, meaning that for expression `foo
&& bar` the `bar` part is only evaluated if `foo` produces a boolean TRUE.

Inko does not provide these operators. Instead, all booleans respond to the
messages `and` and `or`, both which take a closure to evaluate. So instead of
writing `foo && bar` you'd write `foo.and { bar }`. Here are a few more
examples, using Ruby as a comparison:

| Ruby          | Inko
|:--------------|:------------
| `a && b`      | `a.and { b }`
| `a || b`      | `a.or { b }`
| `a && b && c` | `a.and { b }.and { c }`
| `a && b || c` | `a.and { b }.or { c }`

## Sending messages

If you want to send a message to the result of a binary expression, you need to
wrap the expression in parentheses. For example:

```inko
(1 + 2).to_string # => '3'
```

If you leave out the parentheses, the message will be sent to the operand on the
right:

```inko
1 + 2.to_string
```

This results in Inko running `1 + '2'`, producing a compile-time error.

## Indexing operators

There are two operators used for indexing/slicing: `[]` and `[]=`. The `[]`
operator is used for accessing an index, while `[]=` is used for assigning a
value to an index. You use these as follows:

```inko
values[0]
values[0] = 42
```

These operators are just messages, meaning the above example translates to the
following:

```inko
values.[](0)
values.[]=(0, 42)
```

Classes can support these operators by implementing the following methods:

* `def [](index: K) -> R`
* `def []=(index: K, value: V) -> R`

Here `K` is the type of the index, such as an `Integer` or a `String`. `R` is
the return type, and `V` is the type of the value to set.

Instead of implementing these methods manually, you should implement the traits
`std::index::Index` and `std::index::SetIndex`. This ensures types providing
these operators do so using a consistent interface. Let's say we have a type for
storing single characters, defined like so:

```inko
class Chars {
  @chars: Array!(String)

  static def new(chars: Array!(String)) -> Self {
    Self { @chars = chars }
  }
}
```

We want to use it like so:

```inko
let chars = Chars.new(Array.new('a', 'b', 'c'))

chars[0]       # => 'a'
chars[1] = 'd' # => 'd'
```

To achieve this, we implement the traits from `std::index` as follows:

```inko
import std::index::(Index, SetIndex)

class Chars {
  @chars: Array!(String)

  static def new(chars: Array!(String)) -> Self {
    Self { @chars = chars }
  }
}

impl Index!(Integer, String) for Chars {
  def [](index: Integer) -> String {
    @chars[index]
  }
}

impl SetIndex!(Integer, String) for Chars {
  def []=(index: Integer, value: String) -> String {
    @chars[index] = value
  }
}
```

!!! tip
    When implementing the [] method, it's expected for this method to panic when
    used with a key/index that doesn't exist.
