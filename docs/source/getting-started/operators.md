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

## Not-nil operator

There exists on special operator: the postfix `!` operator, also known as the
not-nil operator. This operator is only available to optional types, and only
exists at compile-time. This operator is used to convert a `?T` to a `T`,
without any runtime checks. For example:

```inko
def foo(value: ?Thing) {
  thing.if_true {
    bar(thing!) # Here we say that `thing` is a `Thing`, instead of a `?Thing`
  }
}

def bar(value: Thing) {}
```
