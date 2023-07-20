# Operator overloading

Inko supports a variety of operators (see [this
section](../guides/syntax.md#binary-expressions) for the full list),
and these can be implemented for your own types. In fact, operators are just
regular methods, though the compiler may choose to optimise some of them
whenever possible.

Operators are provided by the various traits found in the module `std.ops`. For
example, the `+` operator is provided by the `Add` trait. Here's how we might
implement this operator for a custom type:

```inko
import std.ops.Add

class Rational {
  let @numerator: Int
  let @denominator: Int
}

impl Add[Rational, Rational] for Rational {
  fn pub +(other: ref Rational) -> Rational {
    Rational {
      @numerator = @numerator + other.numerator,
      @denominator = @denominator + other.denominator
    }
  }
}
```
