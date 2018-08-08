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

[forums]: https://discourse.inko-lang.org
[dev-category]: https://discourse.inko-lang.org/c/development
[new-issue]: https://gitlab.com/inko-lang/inko/issues/new
[style-guide]: https://inko-lang.org/manual/style-guide/
[good-commit-message]: https://chris.beams.io/posts/git-commit/
[coc]: https://inko-lang.org/code-of-conduct/
[rustfmt]: https://github.com/rust-lang-nursery/rustfmt
[clippy]: https://github.com/rust-lang-nursery/rust-clippy
