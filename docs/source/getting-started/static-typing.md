# Static typing

Inko is a statically and strongly-typed language. Thanks to type inference, you
only need to explicitly annotate your types in a few places. This reduces the
amount of bugs in your program, without the cost of having to annotate all types
explicitly.

As a simple example, here is how you'd generate factorial numbers in Ruby, a
dynamically typed language:

```ruby
def fact(n)
  return 1 if n.zero?

  n * fact(n - 1)
end
```

Here is how you'd write the same program in Inko:

```inko
def fact(n: Integer) -> Integer {
  n.zero?.if_true { return 1 }

  n * fact(n - 1)
}
```

## What is static typing

If you're not familiar with the differences between static typing and dynamic
typing, a summary of the difference is as follows:

When you use dynamic typing, the types are not stated explicitly. Instead, at
runtime the language figures out what it's dealing with and if certain
operations are valid. This can lead to unexpected runtime errors when a type is
used to perform an operation it does not support, such as adding a string and an
integer together. Take this Ruby code for example:

```ruby
def add(a, b)
  a + b
end

add(10, 'foo')
```

Running this program will produce the runtime error `TypeError: String can't be
coerced into Integer`, because in Ruby you can't add an integer and a string
together.

When using static typing, the compiler knows all types at compile-time. This
allows the compiler to verify if your program is correct or not, preventing
runtime errors such as those mentioned above. To illustrate, if we convert the
above example to Inko we'd end up with the following:

```inko
def add(a: Integer, b: Integer) -> Integer {
  a + b
}

add(a: 10, b: 'foo')
```

Because Inko is statically typed, this code won't compile, and the compiler will
produce the following error:

```
ERROR: Expected a value of type "Integer" instead of "String"
 --> /tmp/test.inko on line 5, column 12
   |
 5 | add(a: 10, b: 'foo')
   |            ^
```

## Static vs dynamic typing

There has been a long standing debate about what is better: static typing, or
dynamic typing. Inko actually used to be gradual typing, as we believed this to
provide a nice bridge between the two typing strategies. Over time we realised
that gradual typing doesn't bring any benefits over static typing, but does come
with the drawbacks of both static and dynamic typing. In response, we decided to
make Inko fully statically-typed.

When it comes to the debate of static typing versus dynamic typing, we believe
static typing is generally better in the long run for larger programs with
multiple developers. For solo projects or quick scripts, dynamic typing probably
works just as well as static typing.

Inko aims to be a language you can use for projects both small and big, whether
it's a simple program to organise your music collection or a sophisticated web
service. For this reason Inko is statically typed, as we believe static typing
works well for projects of any size.
