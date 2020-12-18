# Iterators

Iterators are objects that can be used to traverse collections such as an
`Array` or a `Map`. Typically iterators are implemented in one of two
ways:

1. Internal iterators: these kind of iterators take care of the iteration
   process and operate using some kind of closure.
2. External iterators: these iterators use some kind of cursor stored
   somewhere and require you to manually advance the iterator.

Both have their benefits and drawbacks. Internal iterators are easy to
implement and typically faster, as they don't require the allocation of
additional data structures.

External iterators can be composed together, suspended, and later resumed.
External iterators can also be turned into internal iterators, while the inverse
is not possible unless a language supports some form of coroutines or
generators.

Inko supports both internal and external iteration. When all you need is to
iterate over some values, you can use internal iteration. If you need to
compose iterators together, you can use external iteration.

When an iterator is created, it's positioned before the first value. To access
the first and any following elements, you _must_ send `advance` to the
iterator, followed by sending `current`. For example:

```inko
let numbers = Array.new(10, 20, 30)
let iter = numbers.iter

iter.advance # => true
iter.current # => 10
```

## Creating iterators manually

Creating an iterator manually requires:

1. A object that tracks the state of the iteration process.
2. An implementation of the `Iterator` trait for this object.

The `Iterator` trait requires that you implement one method: `next`. This method
returns an `Option` to signal the presence or lack of a value.

Let's say we want to create an iterator that yields the first 5 values in an
`Array`, then terminates. We can do so as follows:

```inko
import std::iterator::Iterator

object LimitedIterator!(T) {
  @array: Array!(T)
  @index: Integer

  static def new!(T)(array: Array!(T)) -> LimitedIterator!(T) {
    Self { @array = array, @index = 0 }
  }
}


impl Iterator!(T, Never) for LimitedIterator {
  def next -> ?T {
    (@index == 5).if_true { return Option.none }

    let value = @array.get(@index)
    @index += 1
    value
  }
}
```

The iterator type is defined as `Iterator!(T, E)` with `T` being the type of
values to produce, and `E` being the type to throw; if any. If your iterator
doesn't throw, as is the case above, you can assign `E` to the `Never` type.

With our iterator defined, we can use it like so:

```inko
let mut iterator = LimitedIterator.new(Array.new(1, 2, 3, 4, 5, 6, 7, 8))

iterator.next # => Option.some(1)
iterator.next # => Option.some(2)
iterator.next # => Option.some(3)
iterator.next # => Option.some(4)
iterator.next # => Option.some(5)
iterator.next # => Option.none
```

## Creating iterators using generators

Creating an iterator requires quite a bit of boilerplate code. For non-linear
collections such as graphs, implementing an iterator can also be tricky.

To make this easier, we can use what is called a "generator". A generator is a
method that can be suspended, and resumed later on. We can use generators to
create iterators, without the boilerplate. Using a generator, we can implement
our `LimitedIterator` as follows:

```inko
import std::loop::(while)

def limited_iterator!(T)(values: Array!(T)) => T {
  let mut index = 0

  while({ index < 5 }) {
    yield values[index]
    index += 1
  }
}
```

Here the `=> T` signals that the method `limited_iterator` is a generator,
yielding values of type `T`.

Generator methods can't specify an explicit return type, and can't return values
using the `return` keyword. Thus, the following is invalid:

```inko
import std::loop::(while)

def limited_iterator!(T)(values: Array!(T)) => T {
  let mut index = 0

  while({ index < 5 }) {
    yield values[index]
    index += 1
  }

  return 10
}
```

You _can_ use `return` without providing a value. This is useful if you wish to
stop the generator:

```inko
import std::loop::(while)

def limited_iterator!(T)(values: Array!(T)) => T {
  let mut index = 0

  while({ index < 5 }) {
    yield values[index]
    return
    index += 1
  }
}
```

The last expression of the generator is also ignored. Instead, generator methods
always return an instance of `Generator`.

We can use our generator like so:

```inko
let gen = limited_iterator(Array.new(1, 2, 3, 4, 5, 6, 7, 8))

gen.resume # => Option.some(1)
gen.resume # => Option.some(2)
```

If the generator method throws, the `resume` method re-throws that error. This
means you need to handle it. For example:

```inko
import std::loop::(while)

def limited_iterator!(T)(values: Array!(T)) !! String => T {
  let mut index = 0

  while({ index < 5 }) {
    yield values[index]
    throw 'oops'
    index += 1
  }
}

let gen = limited_iterator(Array.new(1, 2, 3, 4, 5, 6, 7, 8))

try! gen.resume # => Option.some(1)
try! gen.resume # => panic
```

## Generators as iterators

Generators themselves are also iterators. So instead of using `resume`, we can
also use `next`:

```inko
import std::loop::(while)

def limited_iterator!(T)(values: Array!(T)) => T {
  let mut index = 0

  while({ index < 5 }) {
    yield values[index]
    index += 1
  }
}

let iter = limited_iterator(Array.new(1, 2, 3))

iter.next # => Option.some(1)
```
