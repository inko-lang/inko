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

## Inline arrays

Regular arrays (values of type `Array`) are allocated on the heap and support
resizing, such as by using the `Array.push` method. While this is what you want
most of the time, sometimes the cost of these heap allocations is too great. For
example, cryptographic algorithms often create large arrays as part of some
tight loop such that allocating those arrays on the heap can become a
bottleneck.

For such scenarios one can use the `InlineArray` type instead. Unlike `Array`,
an `InlineArray` is allocated on the stack and _doesn't_ support resizing. Such
arrays are created using the syntax `inline [...]`, for example:

```inko
[10, 20, 30]        # => Array[Int]
inline [10, 20, 30] # => InlineArray[Int, 3]
```

To create an `InlineArray` that repeats a value N times, use the syntax
`inline [N * expr]` like so:

```inko
inline [3 * 'developers'] # => inline ['developers', 'developers', 'developers']
```

This syntax is lowered as if the expression (`'developers'` in this case) is
repeated explicitly, meaning the above is lowered into this:

```inko
inline ['developers', 'developers', 'developers']
```

Because an `InlineArray` is allocated on the stack, its semantics are the same
as regular [`inline` types](types/#inline-types). For example, borrowing an
`InlineArray` also borrows each individual value and is thus best avoided. If an
`InlineArray` stores other `inline` values that need to be mutated in-place,
this typically requires you to somehow swap the value instead as methods such as
`InlineArray.get` will create copies of such values upon borrowing them, just as
is the case when storing them in a regular `Array`.

Because borrowing an `InlineArray` copies the array itself and borrows each
value individually, methods such as `Array.slice` aren't also implemented for
`InlineArray` as they could cause unexpected performance problems.

Iterating over an `InlineArray` is done using one of two methods:

- `InlineArray.into_iter` consumes the array and allows you to move values out
  of it
- `InlineArray.indexes` returns an iterator that yields the indexes of the array
  without borrowing it

For example:

```inko
let a = inline [10, 20, 30]
let b = inline [10, 20, 30]

for num in a.into_iter {
  num # => 10, 20, ...
}

for idx in b.indexes {
  b.get(idx) # => Result.Ok(10), Result.Ok(20), ...
}
```

As a rule of thumb, start with using `Array` and only switch to using
`InlineArray` if the use of `Array` becomes a bottleneck, and only if you can
actually measure an improvement by using `InlineArray` instead.

## Drop order

When an `Array` or `InlineArray` is dropped, any remaining values are dropped in
the order in which they are stored in the array. For example, for `[foo, bar]`,
`foo` is dropped before `bar`.
