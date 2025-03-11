---
{
  "title": "Maps"
}
---

A map is a collection of key-value pairs:

```inko
let map = Map.new

map.set('name', 'Alice')
```

There's no dedicated syntax for creating maps, instead you create an instance of
the `Map` type using `Map.new`.

The `Map` type is generic. The keys can be any value that implement the `Hash`
and `Equal` traits, while the values can be anything. All keys and values must
be of the same type, meaning the following maps aren't valid:

```inko
let map1 = Map.new
let map2 = Map.new

map1.set('name', 'Alice')
map1.set(42, 'Bob') # Invalid, as the key must be a String

map2.set('name', 'Alice')
map2.set('age', 42) # Invalid, as the value must be a String
```

## Indexing maps

Maps are indexed using `Map.get` and `Map.get_mut` similar to how [arrays are
indexed](../arrays#indexing-arrays).

The `get` method returns an immutable borrow to the value of a key, while
`get_mut` returns a mutable borrow:

```inko
type Person {
  let @name: String
}

let people = Map.new

people.set('alice', Person(name: 'Alice'))

people.get('alice')     # => Result.Ok(ref Person(name: 'Alice'))
people.get_mut('alice') # => Result.Ok(mut Person(name: 'Alice'))
```

Similar to `Array`, you can panic for missing keys using `Result.or_panic`:

```inko
type Person {
  let @name: String
}

let people = Map.new

people.set('alice', Person(name: 'Alice'))
people.get('alice').or_panic # => ref Person(name: 'Alice')
```

## Adding values

Values are added using `Map.set`:

```inko
let map = Map.new

map.set('name', 'Alice')
```

This method returns the previous value wrapped in an `Option`:

```inko
let map = Map.new

map.set('name', 'Alice') # => Option.None
map.set('name', 'Bob')   # => Option.Some('Alice')
```

## Removing values

Values are removed using `Map.remove`, which returns a `Result` wrapping the
value of an error:

```inko
let map = Map.new

map.set('name', 'Alice')

map.remove('name') # => Result.Ok('Alice')
map.remove('name') # => Result.Error(MissingKey(...))
```

## Iterating over values

You can iterate over values in a map using `Map.iter`, `Map.iter_mut`,
and `Map.into_iter`:

```inko
let map = Map.new

map.set('name', 'Donald Duck')
map.set('city', 'Duckburg')

for pair in map.iter {
  pair.key   # => 'name', 'city'
  pair.value # => 'Donald Duck', 'Duckburg'
}

for pair in map.iter_mut {
  pair.key   # => 'name', 'city'
  pair.value # => 'Donald Duck', 'Duckburg'
}

for pair in map.into_iter {
  pair.key   # => 'name', 'city'
  pair.value # => 'Donald Duck', 'Duckburg'
}
```

Maps preserve the order in which keys are inserted, meaning the order of
iteration is stable.

For more information, refer to the [source
code](https://github.com/inko-lang/inko/blob/main/std/src/std/map.inko).
