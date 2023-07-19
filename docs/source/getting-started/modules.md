# Modules & packages

Inko projects are organised as a collection of "modules". A module is just an
Inko source file you can import into another file using the `import` keyword.
For example:

```inko
import std::stdio
```

This imports the module `std::stdio` and exposes it using the name `stdio`. You
an also import specific symbols, such as types:

```inko
import std::stdio::STDOUT
```

For more information about the syntax of `import` statements, refer to the
[Imports](../guides/syntax.md#imports) syntax documentation.

## Import paths

When importing modules, the compiler looks in the following places to find the
module (in this order):

1. Your project's `src/` directory (see
   [Project structure](../guides/structure.md))
1. The source directories of any dependencies of your project, as specified in
   the `inko.pkg` package manifest
1. Additional source directories specified using the `-i` / `--include` option,
   including the standard library (which is added by default)

If a module isn't found, a compile-time error is produced.

Inko doesn't support importing modules relative to another module.

## Third-party packages

Inko has a built-in package manager, available using the `inko pkg` command and
its subcommands (e.g. `inko pkg add`). Packages are just Git repositories hosted
on a platform such as GitLab or GitHub. There's no central package registry.

### Manifest format

The dependencies for your project are listed in the file `inko.pkg` (called a
"package manifest") in the root directory of your project. The format of this
file is a simple line based format that looks as follows:

```
# This is a comment
require gitlab.com/bob/http 1.0.1 ece1027ada626bddd1efc74ba88a87dbdc19522c
require github.com/alice/json 1.0.0 f3f378ad8ea4b617401b40ace743614995904755
```

Each line is either a comment (when it starts with a `#`), or a command. The
only command supported for now is `require`, which uses the following syntax:

```
require URL VERSION CHECKSUM
```

`URL` is the URL of the Git repository of the dependency. You can use any URL
supported by Git, including local file paths.

`VERSION` is the version of the package in the format `MAJOR.MINOR.PATCH`.

`CHECKSUM` is the SHA1 checksum of the Git commit the version points to. This
value is used to ensure that package contents aren't changed after the package
is published.

### Version selection

Inko's package manager uses [semantic versioning](https://semver.org/) for its
versions, and [minimal version selection](https://research.swtch.com/vgo-mvs)
for version selection.

Minimal version selection means that you list the _minimum_ version of a package
that you need. If multiple packages depend on different versions of the same
package, Inko picks the most recent requirement from that list. Take these
requirements for example:

```
json >= 1.2.3
json >= 1.5.3
json >= 1.8.2
```

Here the most recent version that satisfies all requirements is 1.8.2, so Inko
will pick that version of the "json" package.

Inko supports different major versions of the same package. This makes it
possible for package `foo` to require `json` version 1.8.2 or newer, while
package `bar` requires `json` version 2.0.0 or newer.

Using minimal version selection offers several benefits:

- The implementation is much simpler compared to SAT solvers used for other
  version selecting techniques. Because of this the implementation is also much
  faster.
- You don't need a lock file of sorts that lists all the exact packages and
  versions to use.
- You won't end up using a version of a package that you never tested your code
  against.

For more details we suggest reading through the [article by Russ
Cox](https://research.swtch.com/vgo-mvs) (also linked to above).

### Handling security updates

If a new version of a package is released, Inko ignores it due to the use of
minimal version selection; instead picking the most recent version from the list
of required versions. At first glance this may seem like a bad idea, as you
won't be able to take advantage of security updates of your dependencies.
There's a simple solution to this problem: add the dependency to your `inko.pkg`
with the appropriate minimum version, and Inko takes care of the rest.

## Using Inko's package manager

For a more in-depth overview of the available commands and flags, run `inko pkg
--help`. This also works for the various sub-commands, such as `inko pkg sync
--help`.

### Setting up

Creating an empty `inko.pkg` is done using the `inko pkg init` command.

### Adding packages

Adding a package is done using `inko pkg add`, which takes the package URL and
version to add. For example:

```bash
inko pkg add github.com/inko-lang/example-package 1.2.3
```

This command only adds the package to your `inko.pkg` file, it doesn't install
it into your project.

!!! note
    If the package already exists in the manifest, `inko pkg add` updates the
    existing entry.

!!! note
    Inko doesn't support adding the same dependency multiple times to a project,
    even when using different versions.

### Removing packages

The inverse of `inko pkg add` is the `inko pkg remove` command, which takes a
package URL and removes it from your `inko.pkg`. For example:

```bash
inko pkg remove github.com/inko-lang/example-package
```

### Installing packages

!!! warning
    The `inko pkg sync` command removes all files in the `./dep` directory
    before installing the dependencies, so make sure to not place files not
    managed by Inko in this directory.

Installing dependencies into your project is done using `inko pkg sync`. This
command downloads all the necessary dependencies, selects the appropriate
versions, then installs them in `./dep`. For example:

```
$ inko pkg sync
Updating package cache
  Downloading /home/yorickpeterse/Projects/inko/ipm-test/http 1.0.1
  Downloading /home/yorickpeterse/Projects/inko/ipm-test/json 1.0.0
  Downloading /home/yorickpeterse/Projects/inko/ipm-test/test-package-with-dependency/ 0.5.2
  Downloading /home/yorickpeterse/Projects/inko/ipm-test/test-package 1.1.1
Removing existing dependencies in ./dep
Installing
  /home/yorickpeterse/Projects/inko/ipm-test/json 1.0.0
  /home/yorickpeterse/Projects/inko/ipm-test/http 1.0.1
  /home/yorickpeterse/Projects/inko/ipm-test/test-package 1.1.1
  /home/yorickpeterse/Projects/inko/ipm-test/test-package-with-dependency/ 0.5.2
```

Once installed you can import the dependencies using the `import` statement.

The `dep` directory shouldn't be tracked by Git, so make sure to add it to your
`.gitignore` file like so:

```
/dep
```

### Updating packages

Updating dependencies to their latest version is done using the `inko pkg
update` command. This command either takes a package URL and only updates that
package, or updates all packages if no URL is specified.

By default this command only updates versions to the latest version using the
same major version. For example, if you depend on "json" version 1.2.3, and
1.2.5 is released, `inko pkg update` updates the required version to 1.2.5. When
version 2.0.0 is released, `inko pkg update` ignores it because this version
isn't backwards compatible with version 1. To update across major versions, use
the following:

```bash
inko pkg update --major
```

## Publishing a package

To publish your package, push it to a Git host and add a Git tag with the
version number, prefixed by `v`. For example, if you are releasing 3.8.5, use
`git tag v3.8.5`. Don't forget to push the tag with `git push --tags`.

When naming your package repository, you can prefix the name with `inko-` (e.g.
`inko-http`). This way it's clear the repository is an Inko package, and you
don't have to worry about the repository name conflicting with existing
repositories. When importing the main library module, the compiler strips the
`inko-` prefix. Thus, if the package `inko-http` provides `src/http.inko`, you
can import it using `import http`.

!!! note
    Package names should be restricted to letters, numbers and underscores, with
    an optional `inko-` prefix. If the name includes other characters (e.g. a
    hyphen), you won't be able to import it.
