# Objects

We've briefly covered objects in the [Basic types](basic-types.md) chapter. In
this chapter we'll take a closer look at them.

Objects are similar to classes and structures found in other languages, with one
key difference: the don't support inheritance. Not supporting inheritance and
using traits instead makes for a much more pleasant development experience.

## Defining objects

You can define an object by using the `object` keyword, followed by its name and
a pair of curly braces:

```inko
object Person {

}
```

To create a new instance of an object, we send the `new` message to it:

```inko
Person.new
```

## Attributes

Of course our empty object is not useful, so let's give it some fields to store
data in. We call such fields "attributes", and we define them in the body of an
object. All attributes must be defined in the object before they can be used.
Let's say we want our `Person` type to have two attributes: a name and an age
attribute. Our name will be a `String`, and the age will be an `Integer`. We can
do this as follows:

```inko
object Person {
  @name: String
  @age: Integer
}
```

We refer to attributes using the syntax `@NAME` where `NAME` is the name of the
attribute. Attributes are private to the object, meaning they can't be accessed
directly. Instead, you must define a method that returns the attribute, which
we'll cover below.

When defining an attribute as done above, we can't specify a default value for
the attribute; that's up to the object's constructor method. Each object has a
single constructor method called `init`, which we can define as follows:

```inko
object Person {
  @name: String
  @age: Integer

  def init {
    @name = 'Alice'
    @age = 42
  }
}
```

Like any other method, this method can also take arguments. Any arguments
defined on the `init` method are also available when creating a new object
instance using the `new` message. For example:

```inko
object Person {
  @name: String
  @age: Integer

  def init(name: String, age: Integer) {
    @name = name
    @age = age
  }
}
```

We can now create our instance as follows:

```inko
Person.new(name: 'Alice', age: 42)
```

## Methods

Objects can have two types of methods: instance methods, and static methods.
Instance methods are only available to instances of the object, while static
methods are available to the object itself. When we send the `new` message to
our `Person` object, it ends up calling the static method `new`. This method in
turn calls the instance method `init`.

To define an instance method, use the `def` keyword:

```inko
object Person {
  @name: String
  @age: Integer

  def init(name: String, age: Integer) {
    @name = name
    @age = age
  }

  def name -> String {
    @name
  }
}
```

Here we define the instance method `name`, which returns the `@name` attribute.
We can use this method like so:

```inko
Person.new(name: 'Alice', age: 42).name # => 'Alice'
```

To define a static method, use `static def`:

```inko
object Person {
  @name: String
  @age: Integer

  static def anonymous(age: Integer) -> Person {
    new(name: 'Anonymous', age: age)
  }

  def init(name: String, age: Integer) {
    @name = name
    @age = age
  }

  def name -> String {
    @name
  }
}
```

We can then use it like so:

```inko
Person.anonymous(42).name # => 'Anonymous'
```

Static methods can't refer to attributes, meaning that this is an error:

```inko
object Person {
  @name: String
  @age: Integer

  static def oops -> String {
    @name
  }

  def init(name: String, age: Integer) {
    @name = name
    @age = age
  }
}
```

Both static and instance methods can use `self` to refer to their receiver. In
case of an instance method, that will be the instance the method is called on (a
`Person` instance for our `name` instance method). For static methods, this will
be the object itself (`Person` in this case). You don't need to use `self` if
you want to send a message to the current receiver. This means that this:

```inko
object Person {
  @name: String
  @age: Integer

  def init(name: String, age: Integer) {
    @name = name
    @age = age
  }

  def nickname -> String {
    self.name
  }

  def name -> String {
    @name
  }
}
```

Is the same as this:

```inko
object Person {
  @name: String
  @age: Integer

  def init(name: String, age: Integer) {
    @name = name
    @age = age
  }

  def nickname -> String {
    name
  }

  def name -> String {
    @name
  }
}
```

Sometimes you _do_ need to use `self`. For example, if a method takes an
argument with the same name as another method.
