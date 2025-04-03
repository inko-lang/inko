---
{
  "title": "Conditional compilation"
}
---

While Inko code is portable across platforms and architectures, sometimes you
need to handle differences in the underlying platforms, most commonly when using
the [FFI](../getting-started/ffi).

Inko supports conditional compilation at the `import` level only. This makes it
easier to understand the code, as platform specific code ends up in dedicated
modules.

For example, to import the module `foo` only on amd64 Linux platforms, you'd
write the following:

```inko
import foo if linux and amd64
```

The tags act as an AND, meaning the `import` is only processed if _all_ the
specified tags are available. OR expressions and negations aren't supported, but
OR expressions can be handled by just using separate imports:

```inko
import foo if linux
import foo if mac
```

If a conditionally compiled module includes any errors (e.g. a type error),
those errors won't surface until you compile the code such that the module is
imported based on the available and required build tags. This means that if you
have an `import foo if mac` statement, and the `foo` module contains any errors,
you won't see those errors until you compile your code for/on macOS.

The compiler won't produce any errors for tags it doesn't recognise, meaning the
following import is never processed:

```inko
import foo if kittens
```

## Available build tags

::: note
Custom build tags aren't supported, and we're not sure if or when we'll add
support for custom tags.
:::

The following tags are available:

|=
| Tag
| Meaning
|-
| `amd64`
| The platform is a 64-bits x86 platform
|-
| `arm64`
| The platform is a 64-bits ARM platform
|-
| `freebsd`
| The target OS is FreeBSD
|-
| `mac`
| The target OS is macOS
|-
| `linux`
| The target OS is Linux
|-
| `bsd`
| The target OS is any BSD
|-
| `unix`
| The target OS is any Unix system
|-
| `gnu`
| The target uses the GNU ABI
|-
| `native`
| The target uses the native ABI
|-
| `debug`
| The code is compiled in debug mode.
|-
| `release`
| The code is compiled in release mode.

The bag `bsd` is essentially `(freebsd OR ...)`, while `unix` is essentially
`(freebsd or linux or mac or ...)`.

For Linux targets using glibc, the ABI is `gnu` instead of `native`. For the
time being the ABI tags aren't useful, but in the future we may support both
musl and GNU builds, at which point they can be useful to handle differences
between the two libc implementations.
