---
{
  "title": "Destructors"
}
---

Types can define a method to run when before they are dropped, known as a
"destructor". Destructors are defined by implementing the `Drop` trait from the
`std.drop` module like so:

```inko
import std.drop (Drop)
import std.stdio (Stdout)

class Person {
  let @name: String
}

impl Drop for Person {
  fn mut drop {
    Stdout.new.print('dropping ${@name}')
  }
}

class async Main {
  fn async main {
    Person(name: 'Alice')
  }
}
```

If you run this program, the output is "dropping Alice".

The `drop` method is always a mutable and private methods. If you try to
implement it using `fn pub drop` or `fn drop`, you'll run into a compile-time
error.

## Escaping references

Destructors are mutable methods, which may result in a value that's to be
dropped escaping the `drop` call:

```inko
import std.drop (Drop)
import std.stdio (Stdout)

class Person {
  let @name: String
  let @people: mut Array[ref Person]
}

impl Drop for Person {
  fn mut drop {
    @people.push(self)
    Stdout.new.print('dropping ${@name}')
  }
}

class async Main {
  fn async main {
    let people = []
    let person = Person(name: 'Alice', people: people)
  }
}
```

In such cases, a runtime panic is produced:

```
dropping Alice
Stack trace (the most recent call comes last):
  /var/home/yorickpeterse/Downloads/test.inko:19 in main.Main.main
  /var/home/yorickpeterse/Downloads/test.inko:4 in main.Person.$dropper
Process 'Main' (0x56074450f210) panicked: can't drop a value of type 'Person' as it still has 1 reference(s)
```

In practice you're unlikely to run into cases such as this, but it's worth
keeping in mind.
