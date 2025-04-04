---
{
  "title": "Arrays"
}
---

An array is a collection of values in a fixed order, each of the same size and
type. Arrays are created using square brackets like so:

```inko
[10, 20, 30]
```

Arrays are generic, meaning they can store values of any type as long as all the
values are of the same type:

```inko
[10, 20]       # => OK
['foo', 'bar'] # => OK
[[10], [20]]   # => OK
[10, 'foo']    # => not OK
```

## Indexing arrays

Arrays are indexed using `Array.get` and `Array.get_mut`. Both methods return a
`Result` that wraps the value or an error. There's no dedicated syntax for
indexing.

The `get` method returns an immutable borrow to a value, while `get_mut` returns
a mutable borrow:

```inko
type Person {
  let @name: String
}

let people = [Person(name: 'Alice')]

people.get(0)     # => Result.Ok(ref Person(name: 'Alice'))
people.get_mut(0) # => Result.Ok(mut Person(name: 'Alice'))
```

If you want the program to panic if the index is out of bounds, use
`Result.or_panic`:

```inko

type Person {
  let @name: String
}

let people = [Person(name: 'Alice')]

people.get(0).or_panic # => ref Person(name: 'Alice')
```

## Adding values

Values are typically added using `Array.push`, or `Array.set`. The `push` method
adds a value to the end of the `Array`:

```inko
let nums = []

nums.push(42)
nums # => [42]
```

The `set` method sets a value at a given index:

```inko
let nums = [10]

nums.set(0, 42)
nums # => [42]
```

If the index (the first argument) is out of bounds, `Array.set` panics:

```inko
let nums = []

nums.set(42, 10) # => panic
```

## Removing values

Values can be removed in a variety of ways, such as by using `Array.pop` or
`Array.remove_at`. The `pop` method removes the last value in the array,
wrapping it in an `Option`:

```inko
let nums = [10, 20]

nums.pop # => Option.Some(20)
```

The `remove_at` method removes a value at a given index, shifting the values
that come after it to the left:

```inko
let nums = [10, 20, 30]

nums.remove_at(1) # => Result.Ok(20)
nums              # => [10, 30]
```

## Iterating over values

You can iterate over values in an array using `Array.iter`, `Array.iter_mut`,
`Array.reverse_iter` and `Array.into_iter`:

```inko
let nums = [10, 20, 30]

for num in nums.iter {
  num # => 10, 20, 30
}

for num in nums.iter_mut {
  num # => 10, 20, 30
}

for num in nums.reverse_iter {
  num # => 30, 20 ,10
}

for num in nums.into_iter {
  num # => 10, 20, 30
}
```

The `iter` method returns an iterator that yields immutable borrows to each
value, while `iter_mut` yields mutable borrows. `reverse_iter` yields immutable
borrows but in reverse order. The `into_iter` method in turn yields the values
as-is and takes over ownership of the underlying array.

## Drop order

When an `Array` is dropped, any remaining values are dropped in the order in
which they are stored in the `Array`. For example, for `[foo, bar]`, `foo` is
dropped before `bar`.
