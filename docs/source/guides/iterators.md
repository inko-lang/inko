# Iterators

An Iterator is a type used for iterating over the values of a collection, such
as an `Array` or `Map`. Typically a programming language will use one of two
iterator types:

1. Internal iterators: iterators where the iteration is controlled by a method,
   calling a closure for every value.
2. External iterators: mutable data structures from which you "pull" the next
   value, until you run out of values.

Both have their benefits and drawbacks. Internal iterators are easy to implement
and offer good performance. Internal iterators can't be composed together, they
are eager (the method only returns after consuming all values), and make it
harder to pause and resume iteration later on.

External iterators do not suffer from these problems, as control of iteration is
given to the user of the iterator. This does come at the cost of having to
allocate and mutate an iterator, which may be less efficient compared to
internal iteration.

## Iterators in Inko

Inko primarily uses external iterators, but types may also allow you to use
internal iteration for simple use cases, such as just traversing the values in a
collection. For example, we can iterate over the values of an `Array` by sending
`each` to the `Array`:

```inko
import std::stdio::stdout

Array.new(10, 20, 30).each do (number) {
  stdout.print(number)
}
```

We can also do this using external iteration:

```inko
import std::stdio::stdout

Array.new(10, 20, 30).iter.each do (number) {
  stdout.print(number)
}
```

Using external iterators gives us more control. For example, we can consume the
first value then stop iteration as follows:

```inko
let array = Array.new(10, 20, 30)

array.iter.next # => 10
```

Because external iterators are lazy, this would not iterate over the values `20`
and `30`.

## Implementing iterators

Implementing your own iterators is done in two steps:

1. Create a separate object for your iterator, and implement the
   `std::iterator::Iterator` trait for it.
2. Define a method called `iter` on your object, and return the iterator created
   in the previous step. If an object provides multiple iterators, use a more
   meaningful name instead (e.g. `keys` or `values`).

To illustrate this, let's say we have a simple `LinkedList` type that (for the
sake of simplicity) only supports `Integer` values. First we define an object to
store a single value, called a `Node`:

```inko
object Node {
  @value: Integer
  @next: ?Node

  def init(value: Integer) {
    @value = value

    # The next node can either be a Node, or Nil, hence we use `?Node` as the
    # type. We specify the type explicitly, otherwise the compiler will infer
    # the type of `@next` as `Nil`.
    @next = Nil
  }

  def next -> ?Node {
    @next
  }

  def next=(node: Node) {
    @next = node
  }

  def value -> Integer {
    @value
  }
}
```

Next, let's define our `LinkedList` object that stores these `Node` objects:

```inko
object LinkedList {
  @head: ?Node
  @tail: ?Node

  def init {
    @head = Nil
    @tail = Nil
  }

  def head -> ?Node {
    @head
  }

  def push(value: Integer) {
    let node = Node.new(value)

    @tail.if(
      true: {
        @tail.next = node
        @tail = node
      },
      false: {
        @head = node
        @tail = node
      }
    )
  }
}
```

With our linked list implemented, let's import the `Iterator` trait:

```inko
import std::iterator::Iterator
```

Now we can create our iterator object, implement the `Iterator` trait for it,
and define an `iter` method for our `LinkedList` object:

```inko
# Iterator is a generic type, and defines a single type parameter: the type of
# the values returned by the iterator. In this case our type of the values is
# `Integer`.
object LinkedListIterator {
  @node: ?Node

  def init(list: LinkedList) {
    @node = list.head
  }
}

impl Iterator!(Integer) for LinkedListIterator {
  # This will return the next value from the iterator, if any.
  def next -> ?Node {
    let node = @node

    @node.if_true {
      @node = @node.next
    }

    node
  }

  # This will return True if a value is available, False otherwise.
  def next? -> Boolean {
    @node.if(true: { True }, false: { False })
  }
}

# Now that our iterator object is in place, let's reopen LinkedList and add the
# `iter` method to it.
impl LinkedList {
  def iter -> LinkedListIterator {
    LinkedListIterator.new(self)
  }
}
```

With all this in place, we can use our iterator like so:

```inko
let list = LinkedList.new

list.push(10)
list.push(20)

let iter = list.iter

stdout.print(iter.next.value) # => 10
stdout.print(iter.next.value) # => 20
```

If we want to (manually) cycle through all values, we can do so as well:

```inko
let list = LinkedList.new

list.push(10)
list.push(20)

let iter = list.iter

{ iter.next? }.while_true {
  stdout.print(iter.next.value) # => 10, 20
}
```

Since the above pattern is so common, iterators respond to `each` to make this
easier:

```inko
let list = LinkedList.new

list.push(10)
list.push(20)

let iter = list.iter

iter.each do (node) {
  stdout.print(node.value) # => 10, 20
}
```

## Implementing iterators the easy way

Creating an iterator using the Iterator trait is a bit verbose. To make this
easier, Inko provides the `Enumerator` type in the `std::iterator` module. Using
this type we can implement our linked list iterator as follows:

```inko
import std::iterator::(Enumerator, Iterator)

object Node {
  @value: Integer
  @next: ?Node

  def init(value: Integer) {
    @value = value
    @next = Nil
  }

  def next -> ?Node {
    @next
  }

  def next=(node: Node) {
    @next = node
  }

  def value -> Integer {
    @value
  }
}

object LinkedList {
  @head: ?Node
  @tail: ?Node

  def init {
    @head = Nil
    @tail = Nil
  }

  def head -> ?Node {
    @head
  }

  def push(value: Integer) {
    let node = Node.new(value)

    @tail.if(
      true: {
        @tail.next = node
        @tail = node
      },
      false: {
        @head = node
        @tail = node
      }
    )
  }

  def iter -> Iterator!(Node) {
    let mut node = @head

    Enumerator.new(
      while: { node.if(true: { True }, false: { False }) },
      yield: {
        let current = node

        node = node.next

        current
      }
    )
  }
}
```

Here the iterator code has is reduced to just the following:

```inko
def iter -> Iterator!(Node) {
  let mut node = @head

  Enumerator.new(
    while: { node.if(true: { True }, false: { False }) },
    yield: {
      let current = node

      node = node.next

      current
    }
  )
}
```

We recommend that you use the `Enumerator` type when implementing types, instead
of implementing the `Iterator` trait yourself.
