# Closures

Closures are anonymous functions that have access to local variables defined
outside of a closure. A closure is created using curly braces:

    { 'This is a closure' }

When assigned to a variable a closure can be called by either adding
parenthesis or by explicitly invoking `call` (the former is just syntax sugar
for this):

    let closure = { 'This is a closure' }

    closure()

    closure.call()

Arguments of a closure are specified in parenthesis just before the opening
curly brace:

    let closure = (number: Integer) { number * 2 }

Return types are specified between the parenthesis and the opening curly brace:

    let closure = (number: Integer) -> Integer { number * 2 }

Unlike methods a closure _always_ has a return value, thus the return type can
be omitted. In such a case the return type is determined based on the closure's
body. This means that the above can also be written as this:

    let closure (number: Integer) { number * 2 }

Since `number` is an Integer and `Integer#*` also returns an `Integer` the
language knows that this closure will always return an `Integer`.

Closures do not have their own `self` variable, instead they inherit `self` from
whatever context the closure was created in.
