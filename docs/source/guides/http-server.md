---
{
  "title": "HTTP servers"
}
---

Inko's standard library provides a module for creating HTTP 1.1 servers:
[](std.net.http.server). Such servers support HTTP keep-alive connections,
graceful shutdown in response to a signal, and handle multiple connections
concurrently. Servers support binding to both IP addresses and Unix domain
sockets.

A server created using this module consists of at least two components:

- An instance of [](std.net.http.server.Server) that listens for incoming
  requests
- A type that implements [](std.net.http.server.Handle) and handles the
  incoming request and returns a [](std.net.http.server.Response)

For example, a server that displays "hello" as the response for any request is
implemented as follows:

```inko
import std.net.http.server (Handle, Request, Response, Server)

type async Main {
  fn async main {
    Server.new(fn { recover App() }).start(8_000).or_panic
  }
}

type App {}

impl Handle for App {
  fn pub mut handle(request: mut Request) -> Response {
    Response.new.string('hello')
  }
}
```

The method [](method://std.net.http.server.Server.new) takes a closure that
returns a `uni T` where `T` is some type that implements the `Handle` trait.
This closure is called for each newly established connection.

The method [](method://std.net.http.server.Server.start) binds the server to IP
`0.0.0.0` and port 8000, then waits for and handles incoming connections. This
method doesn't return until the server shuts down.

To verify the server works, run the above program using `inko run` then use the
following [curl](https://curl.se/) command to send a request:

```bash
curl --show-headers http://localhost:8000/
```

If all went well, the output will be the following:

```
HTTP/1.1 200
connection: keep-alive
date: Fri, 10 Oct 2025 20:47:53 GMT
content-length: 5

hello
```

## HEAD requests

The above example always responds with a body, even if the client sends a HEAD
request. To prevent this from happening, use the method
[](std.net.http.server.head_request) to add support for proper HEAD responses:

```inko
import std.net.http.server (Handle, Request, Response, Server, head_request)

type async Main {
  fn async main {
    Server.new(fn { recover App() }).start(8_000).or_panic
  }
}

type App {}

impl Handle for App {
  fn pub mut handle(request: mut Request) -> Response {
    let response = Response.new.string('hello')

    head_request(request, response)
  }
}
```

The `head_request` method expects two arguments: a `mut Request` containing
request details, and the `Response` to return (either as-is or as a HEAD
response).

It's best to always use this method before returning the final response, unless
you explicitly _don't_ want to support HEAD requests.

## Path routing

Always presenting the same response isn't useful, so let's change the example to
show a different response based on the requested path. The HTTP server module
doesn't provide some sort of data structure for routing requests, instead
you pattern match the return value of the method
[](method://std.net.http.server.Request.target):

```inko
import std.net.http.server (Handle, Request, Response, Server, head_request)

type async Main {
  fn async main {
    Server.new(fn { recover App() }).start(8_000).or_panic
  }
}

type App {}

impl Handle for App {
  fn pub mut handle(request: mut Request) -> Response {
    let response = match request.target {
      case [] -> Response.new.string('home')
      case ['about'] -> Response.new.string('about')
      case _ -> Response.not_found
    }

    head_request(request, response)
  }
}
```

If you now visit <http://localhost:8000> you'll see "home", while visiting
<http://localhost/about> results in "about", and all other URLs produce a 404
response.

For more advanced routing needs, such as splitting the request path, use the
[](field://std.net.http.server.Request.path) field. This field stores an
instance of [](std.net.http.server.Path), providing methods such as
[](method://std.net.http.server.Path.starts_with?) and
[](method://std.net.http.server.Path.split_first). For example, if we want to
handle all requests for which the path starts with `kittens`, we can do so as
follows:

```inko
import std.net.http.server (Handle, Request, Response, Server, head_request)

type async Main {
  fn async main {
    Server.new(fn { recover App() }).start(8_000).or_panic
  }
}

type App {}

impl Handle for App {
  fn pub mut handle(request: mut Request) -> Response {
    if request.path.starts_with?('kittens') {
      return Response.new.string('kittens!')
    }

    let response = match request.target {
      case [] -> Response.new.string('home')
      case ['about'] -> Response.new.string('about')
      case _ -> Response.not_found
    }

    head_request(request, response)
  }
}
```

Now requests to URLs such as <http://localhost:8000/kittens> and
<http://localhost:8000/kittens/mittens> produce the response "kittens!".

## Method routing

Besides routing based on the request path, you can also route requests based on
the request method. In the above examples we allowed all request methods, so
let's change that such that _only_ GET and HEAD requests are allowed:

```inko
import std.net.http.server (
  Get, Handle, Head, Request, Response, Server, head_request,
)

type async Main {
  fn async main {
    Server.new(fn { recover App() }).start(8_000).or_panic
  }
}

type App {}

impl Handle for App {
  fn pub mut handle(request: mut Request) -> Response {
    let response = match request.target {
      case [] -> {
        match request.method {
          case Get or Head -> Response.new.string('home')
          case _ -> Response.only_allow([Get, Head])
        }
      }
      case ['about'] -> {
        match request.method {
          case Get or Head -> Response.new.string('about')
          case _ -> Response.only_allow([Get, Head])
        }
      }
      case _ -> Response.not_found
    }

    head_request(request, response)
  }
}
```

This server supports GET and HEAD requests to <http://localhost:8000> and
<http://localhost:8000/about>. For other request methods such as POST,
[](method://std.net.http.server.Response.only_allow) is used to produce an HTTP
405 response that sets the `Allow` header to the list of allowed request
methods. For example, this curl command:

```bash
$ curl -d 'foo' --show-headers http://localhost:8000/
```

Produces this response:

```
HTTP/1.1 405
allow: GET, HEAD
connection: close
date: Fri, 10 Oct 2025 22:00:36 GMT
content-length: 0
```

The symbols `Get` and `Head` in this example are methods that return a
[](std.net.http.Method). You can also use that type directly, though this
results in slightly more verbose code:

```inko
import std.net.http (Method)
import std.net.http.server (Handle, Request, Response, Server, head_request)

type async Main {
  fn async main {
    Server.new(fn { recover App() }).start(8_000).or_panic
  }
}

type App {}

impl Handle for App {
  fn pub mut handle(request: mut Request) -> Response {
    let response = match request.target {
      case [] -> {
        match request.method {
          case Get or Head -> Response.new.string('home')
          case _ -> Response.only_allow([Method.Get, Method.Head])
        }
      }
      case ['about'] -> {
        match request.method {
          case Get or Head -> Response.new.string('about')
          case _ -> Response.only_allow([Method.Get, Method.Head])
        }
      }
      case _ -> Response.not_found
    }

    head_request(request, response)
  }
}
```

::: tip
When routing by both path and method, _first_ match the path _then_ the method,
as shown in the above example. This allows you to produce the correct 405
response for methods that aren't allowed.
:::

## A dedicated routing method

Instead of placing the routing logic directly in the `handle` implementation,
it's recommended to define a `route` method on your `Handle` type and move the
routing logic into this method like so:

```inko
import std.net.http (Method)
import std.net.http.server (Handle, Request, Response, Server, head_request)

type async Main {
  fn async main {
    Server.new(fn { recover App() }).start(8_000).or_panic
  }
}

type App {
  fn mut route(request: mut Request) -> Response {
    match request.target {
      case [] -> {
        match request.method {
          case Get or Head -> Response.new.string('home')
          case _ -> Response.only_allow([Method.Get, Method.Head])
        }
      }
      case ['about'] -> {
        match request.method {
          case Get or Head -> Response.new.string('about')
          case _ -> Response.only_allow([Method.Get, Method.Head])
        }
      }
      case _ -> Response.not_found
    }
  }
}

impl Handle for App {
  fn pub mut handle(request: mut Request) -> Response {
    head_request(request, route(request))
  }
}
```

Using this approach we prevent the `handle` method from becoming a big mess, and
make it easier to find where the routing logic is located.

## Generating HTML

To generate HTML responses we can use the type [](std.html.Html) combined with
the method [](method://std.net.http.server.Response.html):

```inko
import std.html (Html)
import std.net.http.server (Handle, Request, Response, Server, head_request)

type async Main {
  fn async main {
    Server.new(fn { recover App() }).start(8_000).or_panic
  }
}

type App {
  fn mut route(request: mut Request) -> Response {
    match request.target {
      case [] -> {
        Response.new.html(
          Html.new.then(fn (h) {
            h.doctype
            h.head.then(fn (head) { head.title.text('Hello') })
            h.body.then(fn (body) { body.p.text('Hello!') })
          }),
        )
      }
      case _ -> Response.not_found
    }
  }
}

impl Handle for App {
  fn pub mut handle(request: mut Request) -> Response {
    head_request(request, route(request))
  }
}
```

The `Html` type is used to generate an HTML document, and the `Response.html`
method is used to set the response body to this document along with setting the
`Content-Type` header to the correct value.

The `Html` type _doesn't_ build a DOM tree and instead writes its output
directly to an in-memory buffer. This makes it much more efficient compared to
building a tree, but it also means you can't change the document after
generating it (at least not without using an HTML parser).

::: tip
For more information, refer to the documentation of the [](std.html.Html) type.
:::

## HTML forms

When submitting an HTML form, browsers encode the data in one of two formats:
`application/x-www-form-urlencoded` or `multipart/form-data`. The `Request` type
has the following methods for handling such forms:

- [](method://std.net.http.server.Request.url_encoded_form)
- [](method://std.net.http.server.Request.multipart_form)

For example, here's how you'd handle an HTML form that encodes its data using
`application/x-www-form-urlencoded`:

```inko
import std.html (Html)
import std.net.http.server (
  Get, Handle, Head, Request, Response, Server, head_request,
)

type async Main {
  fn async main {
    Server.new(fn { recover App() }).start(8_000).or_panic
  }
}

type App {
  fn show_form -> Response {
    Response.new.html(
      Html.new.then(fn (h) {
        h.doctype
        h.head.then(fn (head) { head.title.text('Form example') })
        h.body.then(fn (body) {
          body.form.attr('action', '/').attr('method', 'POST').then(fn (form) {
            form.label.attr('for', 'username').text('Username: ')
            form.br.close
            form
              .input
              .attr('type', 'text')
              .attr('name', 'username')
              .id('username')
              .close
            form.br.close

            form.label.attr('for', 'password').text('Password: ')
            form.br.close
            form
              .input
              .attr('type', 'password')
              .attr('name', 'password')
              .id('password')
              .close
            form.br.close
            form.br.close

            form.input.attr('type', 'submit').attr('value', 'Login').close
          })
        })
      }),
    )
  }

  fn handle_form(request: mut Request) -> Response {
    let Ok(form) = request.url_encoded_form else return Response.bad_request
    let user = match form.string('username') {
      case Ok(v) if v.size > 0 -> v
      case _ -> return Response.bad_request.string('A username is required')
    }
    let pass = match form.string('password') {
      case Ok(v) if v.size > 0 -> v
      case _ -> return Response.bad_request.string('A password is required')
    }

    Response.new.html(
      Html.new.then(fn (h) {
        h.doctype
        h.head.then(fn (head) { head.title.text('Form example') })
        h.body.then(fn (body) {
          body.p.text('You submitted:')
          body.ul.then(fn (ul) {
            ul.li.text('Username: ${user}')
            ul.li.text('Password: ${pass}')
          })
        })
      }),
    )
  }

  fn mut route(request: mut Request) -> Response {
    match request.target {
      case [] -> {
        match request.method {
          case Get or Head -> show_form
          case Post -> handle_form(request)
          case _ -> Response.only_allow([Get, Head])
        }
      }
      case _ -> Response.not_found
    }
  }
}

impl Handle for App {
  fn pub mut handle(request: mut Request) -> Response {
    head_request(request, route(request))
  }
}
```

The `show_form` method renders a basic HTML login form, while the `handle_form`
method handles the request submitted by the form. If the username or password is
missing or empty, an error response is returned.

Parsing the form may fail such as when the URL encoded data is invalid, thus
`Request.url_encoded_form` returns a `Result[Values, FormError]` instead of just
a `Values`.

Handling multipart forms is a little more tricky because of the format being an
stream of unordered fields rather than a simple list of key-value pairs:

```inko
import std.html (Html)
import std.net.http.server (
  Get, Handle, Head, Request, Response, Server, head_request,
)

type async Main {
  fn async main {
    Server.new(fn { recover App() }).start(8_000).or_panic
  }
}

type App {
  fn show_form -> Response {
    Response.new.html(
      Html.new.then(fn (h) {
        h.doctype
        h.head.then(fn (head) { head.title.text('Form example') })
        h.body.then(fn (body) {
          body
            .form
            .attr('action', '/')
            .attr('method', 'POST')
            .attr('enctype', 'multipart/form-data')
            .then(fn (form) {
              form.label.attr('for', 'username').text('Username: ')
              form.br.close
              form
                .input
                .attr('type', 'text')
                .attr('name', 'username')
                .id('username')
                .close
              form.br.close

              form.label.attr('for', 'password').text('Password: ')
              form.br.close
              form
                .input
                .attr('type', 'password')
                .attr('name', 'password')
                .id('password')
                .close
              form.br.close
              form.br.close

              form.input.attr('type', 'submit').attr('value', 'Login').close
            })
        })
      }),
    )
  }

  fn handle_form(request: mut Request) -> Response {
    let Ok(form) = request.multipart_form else return Response.bad_request
    let mut user = ''
    let mut pass = ''
    let buf = ByteArray.new

    for field_result in form {
      # Parsing the field may fail (e.g. the syntax is invalid), so we need to
      # handle that.
      let Ok(field) = field_result else return Response.bad_request

      # Fields may be returned in any order, so we pattern match against the
      # field name to determine what to do.
      match field.name {
        case 'username' -> {
          let Ok(_) = field.read_all(buf) else return Response.bad_request

          user = buf.drain_to_string
        }
        case 'password' -> {
          let Ok(_) = field.read_all(buf) else return Response.bad_request

          pass = buf.drain_to_string
        }
        # Unknown fields are ignored.
        case _ -> {}
      }
    }

    if user.empty? {
      return Response.bad_request.string('A username is required')
    }

    if pass.empty? {
      return Response.bad_request.string('A password is required')
    }

    Response.new.html(
      Html.new.then(fn (h) {
        h.doctype
        h.head.then(fn (head) { head.title.text('Form example') })
        h.body.then(fn (body) {
          body.p.text('You submitted:')
          body.ul.then(fn (ul) {
            ul.li.text('Username: ${user}')
            ul.li.text('Password: ${pass}')
          })
        })
      }),
    )
  }

  fn mut route(request: mut Request) -> Response {
    match request.target {
      case [] -> {
        match request.method {
          case Get or Head -> show_form
          case Post -> handle_form(request)
          case _ -> Response.only_allow([Get, Head])
        }
      }
      case _ -> Response.not_found
    }
  }
}

impl Handle for App {
  fn pub mut handle(request: mut Request) -> Response {
    head_request(request, route(request))
  }
}
```

::: tip
In the future we may offer an abstraction that makes it easier to handle forms
regardless of how they're encoded.
:::

## Cookies

Support for cookies is provided by the module [](std.net.http.cookie). Parsing
is done using one of the following two methods:

- [](method://std.net.http.cookie.Cookie.parse_request): parses a  `Cookie`
  request header
- [](method://std.net.http.cookie.Cookie.parse_response): parses a `Set-Cookie`
  response header

Generating the values for the cookie headers is done using the following two
methods:

- [](method://std.net.http.cookie.Cookie.to_request): generates a `Cookie`
  header value
- [](method://std.net.http.cookie.Cookie.to_response): generates a `Set-Cookie`
  header value

The following example showcases a server that records the last visit time in a
cookie and adjusts the response body based on the presence of this cookie:

```inko
import std.net.http (Header)
import std.net.http.cookie (Cookie)
import std.net.http.server (Handle, Request, Response, Server, head_request)
import std.time (DateTime, Duration)

type async Main {
  fn async main {
    Server.new(fn { recover App() }).start(8_000).or_panic
  }
}

type App {
  fn index(request: mut Request) -> Response {
    let now = DateTime.local
    let (body, cookie) = match last_visited(request) {
      case Some(cookie) -> {
        let body = 'Your last visit was on ${cookie.value}'

        cookie.value = now.to_iso8601
        (body, cookie)
      }
      case _ -> {
        ('This is your first visit', Cookie.new('last_visit', now.to_iso8601))
      }
    }

    cookie.expires = Option.Some(now + Duration.from_secs(30))
    Response.new.string(body).header(Header.set_cookie, cookie.to_response(now))
  }

  fn last_visited(request: mut Request) -> Option[Cookie] {
    let Ok(val) = request.headers.get(Header.cookie) else return Option.None
    let Ok(cookies) = Cookie.parse_request(val) else return Option.None

    # The `Cookie` header may contain multiple cookies, so we need to make sure
    # we return the right one.
    cookies.into_iter.find(fn (c) { c.name == 'last_visit' })
  }

  fn mut route(request: mut Request) -> Response {
    match request.target {
      case [] -> index(request)
      case _ -> Response.not_found
    }
  }
}

impl Handle for App {
  fn pub mut handle(request: mut Request) -> Response {
    head_request(request, route(request))
  }
}
```

Upon first visiting <http://localhost:8000>, the response will be "This is your
first visit". The next time you visit the response will change to
"Your last visit was on X" where "X" is the time of the last visit. The cookie
is set to expire 30 seconds after generating it.

## Error handling

The `handle` method must return a `Response`, but when producing such a
`Response` an error (e.g. a `Result[Foo, Bar]`) may be produced that needs to be
handled in some way. For example, if the response is meant to show the contents
of a file then we need to handle any errors that may be produced when trying to
open the file.

To understand how error handling is done, it helps to divide errors into one of
two categories: those that should be presented to the user, and those that
shouldn't. Form validation users that are the result of bad user input should
probably be presented to the user, while errors related due to file system
permissions _shouldn't_ be presented as such errors may contain sensitive
information. In other words: errors are either public or private.

For public errors, a `Response` should be built and returned. What that response
contains is entirely up to you.

For private errors you can attach an error message (as a `String`) to a
`Response` using the method [](method://std.net.http.server.Response.error).
This message is _not_ included in the response shown to the user, but can be
used elsewhere (e.g. by logging it).

Private errors are best handled in a custom implementation of
[](method://std.net.http.server.Handle.response). This method is called _after_
returning from the implementation of the `handle` method and is expected to
return the final response. This means we can use this method for adjusting the
response, logging any errors, etc, without cluttering the `handle`
implementation:

```inko
import std.net.http.server (Handle, Request, Response, Server, head_request)
import std.stdio (Stderr)

type async Main {
  fn async main {
    Server.new(fn { recover App() }).start(8_000).or_panic
  }
}

type App {
  fn mut route(request: mut Request) -> Response {
    match request.target {
      case [] -> Response.new.string('home')
      case ['error'] -> Response.bad_request.error('oops!')
      case _ -> Response.not_found
    }
  }
}

impl Handle for App {
  fn pub mut handle(request: mut Request) -> Response {
    head_request(request, route(request))
  }

  fn pub mut response(request: mut Request, response: Response) -> Response {
    match response.error {
      case Some(e) -> Stderr.new.print('encountered an error: ${e}')
      case _ -> {}
    }

    match response.status.to_int {
      case v if v >= 400 and v < 500 -> {
        response.string("The request can't be fulfilled due to a client error")
      }
      case v if v >= 500 and v < 600 -> {
        response.string('An internal server error was encountered')
      }
      case _ -> response
    }
  }
}
```

Using this example, visiting <http://localhost:8000/error> produces an HTTP 400
response and writes a simple error message to STDERR. The response body is also
adjusted based on the response status code.

::: tip
It's recommended that response bodies are set as early (i.e. as close to the
error) as possible, as the `response` method may not have enough information to
determine what the body should be set to, what format it should use, etc.
:::

## Static files

Many HTTP servers need to serve static content such as CSS and Javascript files.
Instead of relying on a reverse-proxy such as Nginx to do so, we can do so using
the type [](std.net.http.server.Directory). This type takes a path to a
directory and serves all static files in this directory and its descendants,
with the correct `Cache-Control` and `Content-Type` headers. It also supports
[range requests][range-req] and [conditional requests][cond-req].

The following example serves all static files in the current working directory
under the path `/static`:

```inko
import std.env
import std.net.http.server (
  Directory, Handle, Request, Response, Server, head_request,
)

type async Main {
  fn async main {
    let pwd = env.working_directory.or_panic

    Server
      .new(fn { recover App(directory: Directory.new(pwd.clone)) })
      .start(8_000)
      .or_panic
  }
}

type App {
  let @directory: Directory

  fn mut route(request: mut Request) -> Response {
    match request.path.split_first {
      case Some(('static', path)) -> return @directory.handle(request, path)
      case _ -> {}
    }

    match request.target {
      case [] -> Response.new.string('home')
      case _ -> Response.not_found
    }
  }
}

impl Handle for App {
  fn pub mut handle(request: mut Request) -> Response {
    head_request(request, route(request))
  }
}
```

For example, if the file `README.md` exists in the working directory then you
can access it using the URL <http://localhost:8000/static/README.md>. An example
response would look something like this:

```
HTTP/1.1 200
content-type: text/markdown
cache-control: public, max-age=2592000, must-revalidate
etag: "4291719073503313452528"
last-modified: Sat, 22 Jun 2024 16:25:03 GMT
accept-ranges: bytes
connection: keep-alive
date: Fri, 10 Oct 2025 22:51:30 GMT
content-length: 429

# The Inko standard library

Inko's standard library is a collection of modules available to every
application. Some types and methods provided by the standard library are
available by default without the need for an explicit `import` (e.g. the `Array`
and `Map` types), while others require an explicit `import` (e.g.
`std.set.Set`).

For more information, refer to the [Inko
manual](https://docs.inko-lang.org/manual/latest/).
```

In this example the method [](method://std.net.http.server.Path.split_first) is
used to get the first component of the request path and all remaining
components. We then match against that first component to see if it equals
`static` and if so pass the rest of the path to
[](method://std.net.http.server.Directory.handle).

::: tip
The `Directory` type protects against path traversal attacks, so you don't have
to worry about exposing information outside the static files directory.
:::

::: warn
The `Directory` type serves _all_ files in the directory and any sub
directories, including hidden files and directories, so make sure these
files don't contain any sensitive information.
:::

## Request logging

For basic request/response logging you can use the type
[](std.net.http.server.Logger):

```inko
import std.net.http.server (
  Handle, Logger, Request, Response, Server, head_request,
)

type async Main {
  fn async main {
    let logger = Logger.new

    Server.new(fn { recover App(logger.clone) }).start(8_000).or_panic
  }
}

type App {
  let @logger: Logger

  fn mut route(request: mut Request) -> Response {
    match request.target {
      case [] -> Response.new.string('home')
      case _ -> Response.not_found
    }
  }
}

impl Handle for App {
  fn pub mut handle(request: mut Request) -> Response {
    head_request(request, route(request))
  }

  fn pub mut response(request: mut Request, response: Response) -> Response {
    @logger.log(request, response)
    response
  }
}
```

::: note
The `Logger` type only supports logging of request/response data. Logging of
custom messages isn't supported.
:::

A `Logger` should only be created once _before_ starting a `Server`, and cloned
for each `Handle` instance using [](method://std.net.http.server.Logger.clone).
This ensures that access to the log output (STDOUT) is synchronized across
request handlers.

The log format is as follows:

```
YEAR-MONTH-DAY:HOUR:MINUTE:SECOND.SUBSECONDZ: IP METHOD PATH+QUERY HTTP/1.1 STATUS "REFERRER" "USER AGENT"
```

For example:

```
2025-10-10T23:00:00.63Z: 127.0.0.1 GET /static/README.md HTTP/1.1 404 "http://localhost:3000/" "Mozilla/5.0 (X11; Linux x86_64; rv:143.0) Gecko/20100101 Firefox/143.0"
```

If the referrer or user agent isn't specified, the value is `-`:

```
2025-10-10T23:00:00.63Z: 127.0.0.1 GET /static/README.md HTTP/1.1 404 "-" "-"
```

## Testing

Testing an HTTP server is done using the
[](std.net.http.test.RequestBuilder) type. This type is used for building a
[](std.net.http.server.Request) and sending it to a type that implements
[](std.net.http.server.Handle):

```inko
import std.net.http (Status)
import std.net.http.server (Handle, Request, Response)
import std.net.http.test (RequestBuilder)
import std.test (Tests)

type Handler {}

impl Handle for Handler {
  fn pub mut handle(request: mut Request) -> Response {
    Response.new.string('hello')
  }
}

type async Main {
  fn async main {
    let tests = Tests.new

    tests.test('Example test', fn (t) {
      let handler = Handler()
      let resp = RequestBuilder.get('/').send(handler)
      let body = ByteArray.new

      t.equal(resp.status, Status.ok)
      t.true(resp.body.reader.read_all(body).ok?)
      t.equal(body.to_string, 'hello')
    })

    tests.run
  }
}
```

## More information

For more information, refer to the documentation of the following:

- [](std.net.http): contains various HTTP building blocks, such as the
  [](std.net.http.Header) type
- [](std.net.http.cookie): handling of cookies for both clients and servers
- [](std.net.http.server): all the logic needed for building an HTTP server
- [](std.html): generating of HTML documents
- [](std.xml): generating of XML documents

[range-req]: https://www.rfc-editor.org/rfc/rfc9110.html#name-range-requests
[cond-req]: https://www.rfc-editor.org/rfc/rfc9110.html#conditional.requests
