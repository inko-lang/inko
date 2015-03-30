# Importing Modules

Unlike other interpreted languages an Aeon program should only have to specify
what files to import in a single file. Enforcing this somehow would mean the
compiler is aware of what exact modules (and maybe versions) are required. This
also makes it easier for the programmer to see what dependencies are being used.
Unused imports should trigger warnings.

The rationale is that in for example Ruby programs a `require` call can occur
anywhere, even inside a method. Given a project doesn't use a Gemspec and/or
Gemfile (which is possible, although rare these days) this makes it _very_ hard
to see what is required to run the program.

For the above an import would probably have to be a compile time instruction,
not a method available for the runtime. This further prevents a programmer from
importing code during runtime.

Using such a system the need for a Gemspec (or similar manifest file) is taken
care of. Instead of specifying dependencies in an external file one can simply
specify them in the main Aeon file. Aeon should also have a notion of
application owners, names, etc. Basically what you'd use a Gemspec for becomes a
core part of the language itself.

A simple example:

    import json/1.0.1

This would import version 1.0.1 of the JSON module. Version formats would be in
the form of:

    version = integer '.' integer ('.' integer)?
    integer = [0-9]+

Imports should also support the ability to alias identifiers and to import
specific ones (instead of all of them). Syntax wise there are a few options. For
example, Rust style imports would look like this:

    import digest::{SHA1,SHA2}
    import digest/1.0::{SHA1,SHA2}

However, the use of curly braces is a bit ugly. I can also do it Python style:

    from digest import SHA1, SHA2
    from digest/1.0 import SHA1, SHA2

This particular syntax would allow for a combination of specific imports and
aliases:

    from digest/1.0 import SHA1 as DigestSHA1, SHA2 as DigestSHA2

Importing everything into the current namespace:

    from digest/1.0 import *

It's a compiler error to refer to different versions of the same module:

    from digest/1.0 import SHA1
    from digest/1.1 import SHA2

This is an error as different versions of the same module can be incompatible,
potentially leading to bugs. The first defined version of a package is
considered to be the valid version.

If no version is given the latest version of a module should be imported. File
system wise this can be as simply as having a "latest" symlink point to the
latest installed version. In other words, this would be valid behaviour wise
(but not syntax wise):

    from digest/latest import SHA1

One can also import submodules:

    from digest::sha1 import SHA1

Which can be combined with versions (which can only be used at the root of a
namespace):

    from digest/1.0::sha1 import SHA1
