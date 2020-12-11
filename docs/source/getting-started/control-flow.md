# Control flow

Inko doesn't use keywords such as `if` and `while` for control flow. Instead,
this is done by sending messages.

An object is considered to be truthy when it evaluates to `true` in a boolean
context. An object is falsy when it evaluates to `false`. By default, all
objects are truthy _except_ for `Nil` and `False`.

## Conditional branching

Conditional branching is done by sending `if_true`, `if_false` or `if` to an
object. This is what you'd use `if` statements for in other languages.

`if_true` and `if_false` both require a single argument: a closure they will
evaluate if needed. `if_true` will call the closure if the receiver is truthy,
otherwise it returns `Nil`. `if_false` does the opposite: it calls the closure
if the receiver is falsy:

```inko
True.if_true { 10 }  # => 10
False.if_true { 10 } # => Nil

True.if_false { 10 } # => Nil
False.if_false { 10 } # => 10
```

You can also use these messages for types other than the `Boolean` type:

```inko
'hello'.if_true { 10 }   # => 10
Array.new.if_true { 10 } # => 10
```

The signature of `if` is as follows:

```inko
def if!(R)(true: do -> R, false: do -> R) -> R
```

In plain English: it takes two closures, of which the return types are `R`
(which is inferred based on the closures passed to `if`), and it returns an `R`.
So if the closure passed to the `true` argument returns an `Integer`, so must
the closure passed to the `false` argument, and `if` itself will also return an
`Integer`.

```inko
True.if(true: { 10 }, false: { 20 })    # => 10
False.if(true: { 10 }, false: { 20 })   # => 20
True.if(true: { 10 }, false: { 'foo' }) # => compile error!
```

## Conditional loops

For conditional loops, send the message `while_true` or `while_false` to a
closure. Both messages also take a closure argument. `while_true` will call the
closure as long as the receiver is truthy, while `while_false` does the
opposite. Take this Ruby code for example:

```ruby
while number < 10
  number += 1
end
```

This translates to the following Inko code:

```inko
{ number < 10 }.while_true {
  number += 1
}
```

Here `{ number < 10 }` is a closure that specifies the condition, while the
closure `{ number += 1 }` specifies what to call when the condition is truthy.
If you want to run the loop while the condition is falsy, replace `while_true`
with `while_false`:

```inko
{ number < 10 }.while_false {
  number += 1
}
```

## Infinite loops

To create an infinite loop, send `loop` to a closure and pass a closure to call
as an argument:

```inko
{
  # This will run forever!
}.loop
```

## Tail recursion

Inko's compiler applies tail-call elimination, allowing for tail-recursive
methods without overflowing the call stack. In fact, this is how Inko implements
loops: the methods `while_true`, `while_false` and `loop` are tail-recursive
methods. For example, `while_true` is implemented as follows:

```inko
impl Block {
  def while_false(block: do) {
    call.if_true { return }
    block.call
    while_false(block)
  }
}
```

A method is tail-recursive if the last expression in its body is a call to
itself. This method is tail-recursive:

```inko
def foo {
  foo
}
```

Because of tail-call elimination, this method doesn't overflow the call stack.

This method is not tail-recursive, because the call to `foo` is _not_ the last
expression:

```inko
def foo {
  foo
  10
}
```

Because it's not tail-recursive, it will cause a stack overflow.

While tail-recursion is useful, in most cases it's easier to send messages such
as `while_true` and `while_false`. When using tail-recursion, any state needed
by a loop iteration must be passed as an argument in the tail call. For example,
if we want to increment a number, pass it to the next iteration, and return if
it reaches 100,  we'd do so as follows:

```inko
def loop_with_number(number = 0) {
  (number == 100).if_true { return }

  loop_with_number(number + 1)
}
```

This results in the loop internals leaking into the method signature. Using
`while_true` this is not the case:

```inko
def loop_with_number {
  let mut number = 0

  { number < 100 }.while_true {
    number += 1
  }
}
```

## Breaking and skipping loops

In various languages, you can skip a single loop iteration using a `continue` or
`next` keyword, while you can break out of a loop with a `break` keyword. These
keywords don't exist in Inko.

One technique for skipping iterations is to wrap the loop body in a conditional:

```inko
{
  something_else.if_true {
    # ...
  }
}.loop
```

An alternative approach is to encode this logic into the loop condition, though
this may not always be possible.

To break out of a loop entirely, you can `return` from the surrounding method.
For example:

```inko
def example {
  let mut number = 0

  {
    (number == 10).if_true {
      # This terminates the loop by returning from the surrounding `example`
      # method.
      return
    }

    number += 1
  }.loop
}
```
