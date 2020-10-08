# Variables

Variables can be used to perform a computation and remember the result. There
are four types of variables: local variables, global variables, constants, and
attributes.

## Local variables

At risk of sounding like Captain Obvious, local variables are variables local to
the scope you define them in. For example, a local variable in a method won't
conflict with a local variable defined outside the method. To define a local
variable, use the `let` keyword:

```inko
let number = 10
```

By default a variable defined using `let` can't be assigned a new value. If you
want to be able to assign a new value, use `let mut`:

```inko
let foo = 10
let mut bar = 10

foo = 20 # This is not valid
bar = 20 # This is valid
```

### Capturing

Closures can can capture local variables from a surrounding scope. For example:

```inko
def example {
  let number = 10

  { number }.call # This will return 10
}
```

Closures can also shadow outer local variables:

```inko
def example {
  let number = 10

  {
    let number = 20

    number
  }.call # This will return 20

  number # This will return 10
}
```

Lambdas can't capture local variables.

### Explicit types

The type of a local variable as inferred from the value assigned to it. If you
want to explicitly state the type, you can do so as follows:

```inko
let number: Integer = 10
```

This is useful if you want the type of the variable to be a trait, while
assigning it an object. For example:

```inko
let value: ToString = 10
```

## Global variables

Global variables can't be created directly, instead the compiler creates these
when you import a module. For example:

```inko
import std::stdio::stdout

stdout
```

Here `stdout` is a global variable. Global variables can be used anywhere in the
module:

```inko
import std::stdio::stdout

def example {
  stdout
}
```

If in a given scope both a local variable and global variable have the same
name, the local variable takes precedence:

```inko
import std::stdio::stdout

def example {
  let stdout = 10

  stdout # This returns 10, not the stdout module
}
```

You can't assign a new value to a global variable:

```inko
import std::stdio::stdout

stdout = 20
```

You _can_ define a module method with the same name as a global variable:

```inko
import std::stdio::stdout

def stdout {}

stdout # This will call the method
```

A global variable is scoped to its surrounding module, so two modules can import
the same symbol without causing any conflicts. This means a better name would be
"module-local variables", but that can be confused with local variables; so we
use "global variables" instead.

## Constants

Constants are variables that start with a capital letter (in the ASCII range
A-Z), or an underscore followed by a capital letter. To define a constant, you
also use the `let` keyword:

```inko
let A = 10
let _A = 10
```

Because constants, well, constant, you can't declare them as mutable using `let
mut`:

```inko
let mut A = 10 # This is invalid
```

This also means you can't assign a new value to a constant:

```inko
let A = 10

A = 20 # This is invalid
```

When you define an object or trait, a constant is defined containing that object
or trait:

```inko
object Person {}

Person # This is a constant that stores our Person object
```

Like global variables, you can use a constant anywhere in the module that
defines it.

### Explicit types

Like local variables, the type of a constant is inferred from its value. And
like local variables, you can specify an explicit type if necessary:

```inko
let VALUE: ToString = 10
```

## Attributes

Attributes are fields of an object, as covered in the [Objects](objects.md)
chapter. Attributes can always be assigned new values:

```inko
object Person {
  @name: String

  def init(name: String) {
    @name = name
  }

  def remove_name {
    @name = ''
  }
}
```

Attributes are only available to the instance methods of an object (including
those added when implementing a trait).

### Explicit types

Attributes always have their type stated explicitly. This means the following is
invalid:

```inko
object Person {
  @name

  def init(name: String) {
    @name = name
  }

  def remove_name {
    @name = ''
  }
}
```
