---
{
  "title": "Compiler optimizations"
}
---

The code compiled by Inko is optimized in two ways: the mid-level IR ("MIR"),
generated after type checking, is optimized in various ways, and various LLVM
optimizations passes are applied when generating machine code.

## Type specialization

The first optimization is type specialization, and this optimization is always
enabled as it's required for generating correct code. This pass takes generic
types and methods and generates versions specialized to the types they are used
with.

You can find more details about how this works in the [design guide about
generics](../design/compiler#generics).

## Inlining

Inko's compiler is able to inline `static` and instance method calls, provided
they're called using static dispatch. `async` methods are never inlined as they
are executed asynchronously.

Methods annotated with the `inline` keyword are _always_ inlined (provided
inlining is enabled in the first place). This means you can use this keyword to
forcefully inline a method, such as is the case for methods such as `Int.+` and
`Float.+`.

Methods without the `inline` keyword are only inlined if the compiler deems this
beneficial. This works by giving each method a score (known as a "weight") based
on its number of instructions. The greater the weight, the larger the method. To
inline methods, the inliner builds a graph of all methods called and processes
them in bottom-up order. For each method (= the "caller"), the inliner looks at
each called method (= the "callee"). If the combined size of the caller and
callee is below the inlining threshold, the callee is inlined into the caller.
The inliner might also inline a method if it's called in only a few places, even
if doing so would exceed the caller's inline threshold.

The algorithm used for calculating the method weights and the inlining threshold
are unspecified, can't be configured by the user, and are subject to change.

## Escape analysis

As part of the inlining process the compiler also applies [escape
analysis](https://en.wikipedia.org/wiki/Escape_analysis). The basic idea is
simple: if the compiler can statically determine if a heap allocated type
doesn't outlive the scope it's allocated in, it can be allocated on the stack
instead. Of course the actual implementation is a little more complicated than
that.

When inlining methods the work is performed inside-out, so if method `A` calls
method `B` then `B` is processed before `A`. Performing escape analysis in the
same pass/traversal brings two benefits:

1. Callers can take advantage of information produced for the callees
1. We don't need to perform the same traversal twice

The type of escape analysis used is a form of "interprocedural" analysis: data
produced for one method is used by other methods, instead of processing each
method in isolation.

Inko's escape analysis is quite effective, with the promotion percentage ranging
from 50% all the way up to 85% depending on the project. "promotion" here
refers to a heap allocated value that's _promoted_ to a stack allocated value.

The code for escape analysis lives in the `compiler::mir::escape` module, so
refer to that for more details.

## Dead code removal

It's possible some methods are no longer called if the inliner inlined them into
all their call sites. The compiler detects such methods and removes them,
reducing the size of the final executable.

Methods that might be the target of a dynamic dispatch call site are only
removed if the compiler can statically determine they're never called. This
means that methods defined through traits might not be removed, even if they're
never called.

The compiler also detects and removes instructions that don't have any side
effects and of which the result is unused. This is limited to simple
instructions such as those used for integer and string literals, allocations,
and a few others.
