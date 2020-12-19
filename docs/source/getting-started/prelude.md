# The prelude

The prelude is a set of types and methods automatically added to your Inko
modules, removing the need for manually importing these symbols.

The following types are added to the prelude:

* `std::map::Map`
* `std::option::Option`
* `std::range::Range`

The following module methods are added to the prelude:

* `std::loop.while`
* `std::loop.loop`

Since the above symbols are added by the prelude, importing any symbols with the
same name results in a compile-time error.
