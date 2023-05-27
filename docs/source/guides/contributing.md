# Contributing to Inko

Thank you for contributing to Inko!

Before you continue reading this document, please familiarise yourself with our
[Code of Conduct](https://inko-lang.org/code-of-conduct/). All contributors are
expected to adhere to this code of conduct.

## Creating issues

### Reporting bugs

Bugs should be reported at the appropriate issue tracker. For example, bugs for
Inko itself should be reported in the project
[inko-lang/inko](https://github.com/inko-lang/inko/issues), while bugs
specific to Inko's version manager should be reported in the project
[inko-lang/ivm](https://github.com/inko-lang/ivm/issues). Here are a few of
our projects:

| Project            | Description               | Issue tracker
|:-------------------|:--------------------------|:----------------------------
| inko-lang/inko     | The main project for Inko | <https://github.com/inko-lang/inko/issues>
| inko-lang/ivm      | Inko's version manager    | <https://github.com/inko-lang/ivm/issues>
| inko-lang/inko.vim | Vim integration for Inko  | <https://github.com/inko-lang/inko.vim/issues>
| inko-lang/website  | The Inko website          | <https://github.com/inko-lang/website/issues>

For an up to date list, take a look at the [inko-lang GitHub
group](https://github.com/inko-lang).

Before reporting a bug, please make sure an issue doesn't already exist for the
bug. To find all reported bugs, filter the list of issues using the `bug` label.
Duplicate issues may be closed without warning. If you found an existing issue,
please don't reply with comments such as "Me too!" and "+1". Instead, click on
the thumbs up Emoji displayed when viewing the issue.

If no issue exists for the bug, please create a new issue. When reporting an
issue, please use the "bug" issue template. You can find this template in the
"Description" dropdown.

When selected, the issue description will be filled in with the template. Please
fill in all the necessary fields and sections. The more information you provide,
the easier it will be for maintainers to help you out.

### Feature requests

Before creating an issue to request a new feature, make sure no issue already
exists. Features use the label `feature`. Similar to existing bug reports,
please use the thumbs up Emoji if you'd like to see the feature implemented;
instead of replying with comments such as "+1".

If no issue exists, you can create one using the "feature" issue template. When
requesting a new feature, please fill in all the sections of the issue template.
Also please include examples of how your feature would be used, details about
how other languages implement the feature (if applicable), and so on.

## Submitting changes

!!! note
    Before submitting code changes, please note that we only accept merge
    requests for issues labelled as "Accepting contributions".

To submit changes to Inko, you'll need a local Git clone of the repository. If
you want to contribute to inko-lang/inko, you need to [build Inko from
source](../getting-started/installation.md#building-from-source).

### Rust code

Rust code is formatted using [rustfmt](https://github.com/rust-lang/rustfmt),
and [clippy](https://github.com/rust-lang/rust-clippy) is used for additional
linting. You can install both using [rustup](https://rustup.rs/) as follows:


```bash
rustup component add rustfmt clippy
```

For rustfmt we recommend setting up your editor so it automatically formats your
code. If this isn't possible, you can run it manually like so:

```bash
rustfmt --emit files */src/lib.rs */src/main.rs
```

Clippy can be run using the command `cargo clippy`. Unit tests are run using the
`cargo test` command.

### Inko code

For contributing changes to Inko source code, please follow [the Inko style
guide](style-guide.md). We don't have any tools yet to enforce the style guide,
so this is done manually during code review.

Unit tests for Inko are located in `std/test` and are named `test_X.inko`,
where `X` is the module to test. For example, the tests for `std::string` are
located in `std/test/std/test_string.inko`. Test modules are structured as
follows:

```inko
import std::test::Tests

fn pub tests(t: mut Tests) {
  t.test('Test name') fn (t) {
    t.equal(foo, bar)
  }
}
```

When adding a new test module, follow this structure then add it to
`std/test/main.inko`, following the same style as the existing tests.

To run the stdlib tests:

1. Enter the std directory `cd std`
2. Run the tests using `cargo run -p inko --release -- test`

### Shell scripts

Some parts of our continuous integration setup depend on some shell scripts.
These scripts are checked using [shellcheck](https://www.shellcheck.net/). Lines
are wrapped at 80 characters per line.

### Documentation

Documentation is written in Markdown. For the manual we use
[Vale](https://docs.errata.ai/vale/about) to enforce a consistent style. We
recommend setting up your editor so it automatically checks Markdown using Vale.
If this isn't possible, you can run Vale manually like so:

```bash
vale docs
```

For English, we use British English instead of American English.

### Writing commit messages

Writing a good commit message is important. Code and tests help explain what is
implemented and how it works. Commit messages help explain the thought process
that went into these changes, who is involved in the work, what work may be
related, and more. This information is invaluable to maintainers, for example
when debugging a bug introduced some time in the past.

Commit messages follow these rules:

1. The first line is the subject, and must not be longer than 50 characters.
1. The second line is empty.
3. The third and following lines make up the commit body. These lines must not
   be longer than 72 characters.

The second and all following lines can be left out if the subject is explanatory
enough.

The first line of a commit message is used when updating a project's changelog.

Here is an example of a good commit message:

```
Lint commits using gitlint

This adds a CI job called `lint:gitlint` that lints commits using
gitlint (https://github.com/jorisroovers/gitlint). In addition, we
include three custom linters to help enforce good commit message
practises. For example, commit bodies may not be longer than 72
characters _unless_ they exceed this limit due to a URL.
```

Another example can be seen in commit
[b64323](https://github.com/inko-lang/inko/commit/b64323fe288e2c21aeff268ca27fa47b0ed8732d).

When writing commit messages, please refrain from including tags like "feat:",
"bug:", and others recommended by projects such as [Conventional
Commits](https://www.conventionalcommits.org/). Such information is not helpful
when looking at commits, and is better suited for issues and merge requests.
Keeping the above rules in mind, it also reduces the amount of space available
per line; the subject in particular.

These rules are enforced using
[gitlint](https://github.com/jorisroovers/gitlint), which runs as part of our
continuous integration setup. If needed, you can run gitlint manually like so:

```bash
gitlint
```

To run gitlint for the last 10 commits:

```bash
gitlint --commits 'HEAD~10..HEAD'
```

For more details about writing commit messages, take a look at
[this article](https://chris.beams.io/posts/git-commit/).

### Changelog entries

The changelog is generated from Git commits. To include a commit in the
changelog, add the `Changelog` trailer to the end of the commit. This trailer
can be set to the following values:

- `added`: for new features
- `fixed`: for bug fixes
- `changed`: for something that changed but isn't necessarily a bug fix
- `other`: for any other kind of change
- `performance`: improvements to the performance of Inko

For example:

```
This is the subject of a commit

This is the body of the commit message, providing more details about the
changes.

Changelog: fixed
```
