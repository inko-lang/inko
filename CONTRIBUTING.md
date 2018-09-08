# Contributing

Thank you for your interest in contributing to Inko! There are many ways you can
contribute, and this guide should help you get started.

As a reminder, all contributors are expected to follow our [Code of
Conduct][coc].

## Feature requests

A feature request is broken up into three stages:

1. Idea
1. Proposal
1. Implementation

### Idea

In the idea you propose a (rough) idea of your feature. The goal of this stage
is to gather feedback from the community, before drafting up a more refined
proposal. These ideas should be submitted to the [forums][forums], specifically
in the [Development category][dev-category].

The idea stage can be skipped if you already have a solid idea and proposal.

### Proposal

In this stage you propose the idea to the Inko developers by [submitting an
issue][new-issue]. When creating an issue, use the "feature" issue template.
Feature requests that don't use this template will be closed without warning.

When submitting a feature request, keep in mind that every addition has to be
maintained for a long period of time (five to ten years at least) by the Inko
developers. This means that every addition should be considered carefully, and
should only be added if it is useful to the wider community. Features for
specific or personal use cases are better suited for third-party libraries.

### Implementation

A feature will reach the implementation stage once it has been accepted by the
developers. The time it takes for a feature to be implemented may vary greatly.
If you want to ensure the feature becomes available as soon as possible, it's
best to submit a merge request that implements the feature yourself.

When submitting a merge request that implements a feature, use the "feature"
merge request template.

Merge requests that implement a feature are required to also include
documentation and tests for the new feature.

## Bug reports

When reporting a bug, use the "bug" issue template. Bug reports that don't use
this template will be closed without warning.

## Submitting merge requests

When submitting a merge request, make sure to use the appropriate merge request
template, if available. Merge requests that fix a bug should use the "bug"
template, while merge requests that implement a feature should use the "feature"
template.

Merge requests may only implement a single feature, or fix a single bug. This
makes it easier to review merge requests.

## Writing Inko code

All Inko code written must follow the [Inko style guide][style-guide].

## Writing commit messages

Each commit must contain a well written commit message. The article
[How to Write a Git Commit Message][good-commit-message] describes in detail
what a good commit message is. In short:

* The first line of a commit message _must_ be no more than 50 characters long,
  and must not end with a period (`.`).
* The second line of a commit message should be empty.
* The third and any following lines are used for a more detailed description.
  These lines _must_ be no more than 72 characters per line.

The detailed description _can_ be left out, but commit messages without one will
not be included in the changelog. Commits that implement a feature or fix a bug
_must_ include a detailed description, and _must_ reference the issue describing
the feature or bug.

Don't include meta data such as "type: bug" in the commit message, such data
belongs in the issue or merge request, not the commit.

## Writing Rust code

When writing Rust code for the VM, the use of [rustfmt][rustfmt] is required.
We also recommend installing and using [Clippy][clippy].

## Writing Inko tests

Tests for Inko's runtime are written in Inko itself, using the module
`std::test`. Tests are located in `runtime/tests/test`, and mirror the structure
of the files in `runtime/src`. Test files start with `test_`. This means that
the tests for `std::integer` are located in
`runtime/tests/test/std/test_integer.inko`.

The basic layout of a test looks like the following:

```inko
import std::test
import std::test::assert

test.group('std::module::TypeName.method_name') do (g) {
  g.test('The operation we are testing') {
    # ...
  }
}
```

The name of the group is the fully qualified name of the method or type that is
being tested. This means that if you are adding tests for `Integer.to_integer`,
the group name would be `std::integer::Integer.to_integer`.

The name of the test should briefly describe the operation that is being tested,
not the result that is produced. For example, when writing a test to convert an
`Integer` to a `Float` we would use the following test description:

> Converting an Integer to a Float

Instead of:

> Returns a Float

Other things to keep in mind when writing test descriptions:

* Do not include a period at the end of a test description
* Do not start test descriptions with "it", such as `test 'it returns 10'`
* Keep tests as minimal as possible
* Use single quotes for group and test descriptions, only use double quotes if
  the description itself includes a single quote
* Do not write a test that just asserts if type `A` implements trait `B`,
  instead write tests for the _methods_ implemented

A simple example:

```inko
import std::test
import std::test::assert

test.group('std::integer::Integer.to_integer') do (g) {
  g.test('Converting an Integer to another Integer') {
    assert.equal(10.to_integer, 10)
  }
}

test.group('std::integer::Integer./') do (g) {
  g.test('Dividing an Integer by an Integer') {
    assert.equal(10 / 2, 5)
  }
}
```

[forums]: https://discourse.inko-lang.org
[dev-category]: https://discourse.inko-lang.org/c/development
[new-issue]: https://gitlab.com/inko-lang/inko/issues/new
[style-guide]: https://inko-lang.org/manual/style-guide/
[good-commit-message]: https://chris.beams.io/posts/git-commit/
[coc]: https://inko-lang.org/code-of-conduct/
[rustfmt]: https://github.com/rust-lang-nursery/rustfmt
[clippy]: https://github.com/rust-lang-nursery/rust-clippy
