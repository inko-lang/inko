---
{
  "title": "HTTP clients"
}
---

Besides providing support for creating [HTTP servers](http-server), the standard
library also provides a module for sending HTTP 1.1 requests:
[](std.net.http.client). This module provides the type
[](std.net.http.client.Client), which is an HTTP 1.1 client that supports HTTP,
HTTPS and Unix domain socket requests.

## Getting started

To send a request, we need two things:

- An instance of [](std.net.http.client.Client) to send the request
- An instance of [](std.uri.Uri) that specifies _where_ to send the request to

For example, here's how to send a GET request to <https://httpbun.com/get> and
read its response:

```inko
import std.net.http.client (Client)
import std.stdio (Stdout)
import std.uri (Uri)

type async Main {
  fn async main {
    let client = Client.new
    let uri = Uri.parse('https://httpbun.com/get').or_panic
    let res = client.get(uri).send.or_panic
    let buf = ByteArray.new
    let _ = res.body.read_all(buf).or_panic

    Stdout.new.print(buf)
  }
}
```

[](method://std.net.http.client.Client.new) returns a new HTTP client.
[](method://std.uri.Uri.parse) parses a URI from a [](std.string.String) and
returns a `Result[Uri, Error]`. The method
[](method://std.net.http.client.Client.get) returns a
[](std.net.http.client.Request) for building a GET request, and
[](method://std.net.http.client.Request.send) sends the request, without
including a body. The return value is an instance of [](std.net.http.Response),
and the field [](field://std.net.http.Response.body) contains the response body,
which implements the [](std.io.Read) trait.

The output of this example is as follows:

```json
{
  "method": "GET",
  "args": {},
  "headers": {
    "Accept-Encoding": "gzip",
    "Host": "httpbun.com",
    "User-Agent": "inko/0.18.1 (https://inko-lang.org)",
    "Via": "1.1 Caddy"
  },
  "origin": "86.93.96.67",
  "url": "https://httpbun.com/get",
  "form": {},
  "data": "",
  "json": null,
  "files": {}
}
```

## Methods

The [](std.net.http.client.Client) type defines the following methods for
generating HTTP requests along with the HTTP request method used:

- [](method://std.net.http.client.Request.get): GET
- [](method://std.net.http.client.Request.post): POST
- [](method://std.net.http.client.Request.put): PUT
- [](method://std.net.http.client.Request.delete): DELETE
- [](method://std.net.http.client.Request.head): HEAD
- [](method://std.net.http.client.Request.request): for all other request
  methods (e.g. TRACE)

## Headers

Extra request headers are added using the
[](method://std.net.http.client.Request.header) method. This method takes
ownership of its receiver:

```inko
import std.net.http (Header)
import std.net.http.client (Client)
import std.stdio (Stdout)
import std.uri (Uri)

type async Main {
  fn async main {
    let client = Client.new
    let uri = Uri.parse('https://httpbun.com/get').or_panic
    let res = client
      .get(uri)
      .header(Header.user_agent, 'custom agent')
      .header(Header.new('custom-header'), 'custom-value')
      .send
      .or_panic
    let buf = ByteArray.new
    let _ = res.body.read_all(buf).or_panic

    Stdout.new.print(buf)
  }
}
```

The output is as follows:

```json
{
  "method": "GET",
  "args": {},
  "headers": {
    "Accept-Encoding": "gzip",
    "Custom-Header": "custom-value",
    "Host": "httpbun.com",
    "User-Agent": "custom agent",
    "Via": "1.1 Caddy"
  },
  "origin": "86.93.96.67",
  "url": "https://httpbun.com/get",
  "form": {},
  "data": "",
  "json": null,
  "files": {}
}
```

## Query strings

The method [](method://std.net.http.client.Request.query) is used to add
query string parameters to the request:

```inko
import std.net.http.client (Client)
import std.stdio (Stdout)
import std.uri (Uri)

type async Main {
  fn async main {
    let client = Client.new
    let uri = Uri.parse('https://httpbun.com/get').or_panic
    let res = client
      .get(uri)
      .query('name', 'Alice')
      .query('age', '42')
      .send
      .or_panic
    let buf = ByteArray.new
    let _ = res.body.read_all(buf).or_panic

    Stdout.new.print(buf)
  }
}
```

The output is as follows:

```json
{
  "method": "GET",
  "args": {
    "age": "42",
    "name": "Alice"
  },
  "headers": {
    "Accept-Encoding": "gzip",
    "Host": "httpbun.com",
    "User-Agent": "inko/0.18.1 (https://inko-lang.org)",
    "Via": "1.1 Caddy"
  },
  "origin": "86.93.96.67",
  "url": "https://httpbun.com/get?name=Alice&age=42",
  "form": {},
  "data": "",
  "json": null,
  "files": {}
}
```

## Bodies

To include a body in the request, use
[](method://std.net.http.client.Request.body):

```inko
import std.net.http.client (Client)
import std.stdio (Stdout)
import std.uri (Uri)

type async Main {
  fn async main {
    let client = Client.new
    let uri = Uri.parse('https://httpbun.com/post').or_panic
    let res = client.post(uri).body('request body').or_panic
    let buf = ByteArray.new
    let _ = res.body.read_all(buf).or_panic

    Stdout.new.print(buf)
  }
}
```

The output is as follows:

```json
{
  "method": "POST",
  "args": {},
  "headers": {
    "Accept-Encoding": "gzip",
    "Content-Length": "12",
    "Host": "httpbun.com",
    "User-Agent": "inko/0.18.1 (https://inko-lang.org)",
    "Via": "1.1 Caddy"
  },
  "origin": "86.93.96.67",
  "url": "https://httpbun.com/post",
  "form": {},
  "data": "request body",
  "json": null,
  "files": {}
}
```

## HTML forms

Generating and sending HTML form data is done using
[](method://std.net.http.client.Request.url_encoded_form) or
[](method://std.net.http.client.Request.multipart_form), depending on the
encoding type that's necessary. For example, a URL encoded form is built and
sent as follows:

```inko
import std.net.http.client (Client)
import std.stdio (Stdout)
import std.uri (Uri)

type async Main {
  fn async main {
    let client = Client.new
    let uri = Uri.parse('https://httpbun.com/post').or_panic
    let form = client.post(uri).url_encoded_form

    form.add('name', 'Alice')
    form.add('age', '42')

    let res = form.send.or_panic
    let buf = ByteArray.new
    let _ = res.body.read_all(buf).or_panic

    Stdout.new.print(buf)
  }
}
```

The output is as follows:

```json
{
  "method": "POST",
  "args": {},
  "headers": {
    "Accept-Encoding": "gzip",
    "Content-Length": "17",
    "Content-Type": "application/x-www-form-urlencoded",
    "Host": "httpbun.com",
    "User-Agent": "inko/0.18.1 (https://inko-lang.org)",
    "Via": "1.1 Caddy"
  },
  "origin": "86.93.96.67",
  "url": "https://httpbun.com/post",
  "form": {
    "age": "42",
    "name": "Alice"
  },
  "data": "",
  "json": null,
  "files": {}
}
```

## Keep-alive connections

After establishing a connection, the connection is kept alive until the server
disconnects the connection (e.g. due to it being idle for too long). Connections
are scoped per URI scheme, host and port. This means that sending a request to
`http://foo` and `https://foo` results in _two_ connections.

## HTTPS requests

A `Client` supports both HTTP and HTTPS requests. The TLS configuration used for
performing HTTPS requests is initialized as needed and stored in the field
[](field://std.net.http.client.Client.tls), unless the field already contains a
TLS configuration object.

To specify a custom TLS configuration, create an instance of
[](std.net.tls.ClientConfig) and store it in the
[](field://std.net.http.client.Client.tls) field as an `Option.Some`:

```inko
import std.net.http.client (Client)
import std.net.tls (ClientConfig)
import std.stdio (Stdout)
import std.uri (Uri)

type async Main {
  fn async main {
    let client = Client.new

    client.tls = Option.Some(ClientConfig.builder.new.get)

    let uri = Uri.parse('https://httpbun.com/get').or_panic
    let res = client.get(uri).send.or_panic
    let buf = ByteArray.new
    let _ = res.body.read_all(buf).or_panic

    Stdout.new.print(buf)
  }
}
```

In this example we use `ClientConfig.new` to create a configuration object that
uses the system's certificates. While this is the same as a `Client` does
automatically (if needed), it illustrates how one may specify a custom TLS
configuration.

::: tip
In other words: if you just want to use the system's certificates you _don't_
need to assign the `tls` field yourself.
:::

## Following redirects

If the request method is GET, HEAD, OPTIONS or TRACE, redirects are followed
automatically. Unsafe redirects (e.g. a redirect from an HTTPS to HTTP URL)
result in a [](constructor://std.net.http.client.Error.InsecureRedirect) error.

The maximum number of redirects followed is defined by the
[](field://std.net.http.client.Client.max_redirects) field, and defaults to a
maximum of 5 redirects. Upon encountering too many redirects, a
[](constructor://std.net.http.client.Error.TooManyRedirects) error is returned.

When sending a `multipart/form-data` request generated using
[](method://std.net.http.client.Request.multipart_form), redirects are not
followed _regardless_ of the request method, as the streaming nature of multipart
forms makes it impossible to do so reliably in a generic way. For example, if
such a form field's value is populated from a file, the file's cursor would need
to rewind back to the start but a `Client` has no way of doing so.

## Cookies

To send cookies along with a request, create a [](std.net.http.cookie.Cookie)
instance and use it to populate the `Cookie` header accordingly:

```inko
import std.net.http (Header)
import std.net.http.client (Client)
import std.net.http.cookie (Cookie)
import std.stdio (Stdout)
import std.uri (Uri)

type async Main {
  fn async main {
    let client = Client.new
    let uri = Uri.parse('https://httpbun.com/cookies').or_panic
    let name = Cookie.new('name', 'Alice')
    let age = Cookie.new('age', '42')
    let res = client
      .get(uri)
      .header(Header.cookie, '${name.to_request}; ${age.to_request}')
      .send
      .or_panic
    let buf = ByteArray.new
    let _ = res.body.read_all(buf).or_panic

    Stdout.new.print(buf)
  }
}
```

The output is as follows:

```json
{
  "cookies": {
    "age": "42",
    "name": "Alice"
  }
}
```

::: note
Support for client cookie jars is not yet provided. Refer to [this
issue](https://github.com/inko-lang/inko/issues/877) for more details.
:::

## WebSockets

A [WebSocket](https://www.rfc-editor.org/rfc/rfc6455.html) connection is
established using [](method://std.net.http.client.Client.websocket) and
[](method://std.net.http.client.WebsocketRequest.send):

```inko
import std.fmt (fmt)
import std.net.http.client (Client)
import std.stdio (Stdout)
import std.uri (Uri)

type async Main {
  fn async main {
    let client = Client.new
    let uri = Uri.parse('https://echo.websocket.org').or_panic
    let (sock, _response) = client.websocket(uri).send.or_panic

    let _ = sock.receive.or_panic
    let _ = sock.text('hello').or_panic

    Stdout.new.print(fmt(sock.receive))
  }
}
```

Building and running this program results in the following output:

```
Ok(Text("hello"))
```

## Testing

Testing an HTTP client is done using the [](std.net.http.test.Server) type. This
type is an HTTP server that responds to requests using pre-defined mock
responses:

```inko
import std.net.http (Status)
import std.net.http.client (Client)
import std.net.http.server (Response)
import std.net.http.test (Server)
import std.test (Tests)
import std.uri (Uri)

type async Main {
  fn async main {
    let tests = Tests.new

    tests.test('Example test', fn (t) {
      let server = Server.new(t, fn (srv) {
        srv.get('/').then(fn { Response.new.string('hello') })
      })

      let client = Client.new

      server.prepare_client(client)

      let uri = Uri.parse('http://example.com').or_panic
      let resp = client.get(uri).send.or_panic
      let body = ByteArray.new
      let _ = resp.body.read_all(body).or_panic

      t.equal(resp.status, Status.ok)
      t.equal(body.to_string, 'hello')
    })

    tests.run
  }
}
```

If a mock's criteria aren't met, the test fails:

```inko
import std.net.http.server (Response)
import std.net.http.test (Server)
import std.test (Tests)

type async Main {
  fn async main {
    let tests = Tests.new

    tests.test('Example test', fn (t) {
      let _server = Server.new(t, fn (srv) {
        srv.get('/').then(fn { Response.new.string('hello') })
      })
    })

    tests.run
  }
}
```

Running this test produces the following:

```
F

Failures:

1. Test: Example test
   Line: test_http.inko:11

     expected: this request to be received exactly once:

               GET /

          got: 0 requests

Finished running 1 tests in 0 milliseconds, 1 failures, seed: -4558206327035014586
```

Similarly, requests for which no mocks exist also result in test failures:

```inko
import std.net.http (Status)
import std.net.http.client (Client)
import std.net.http.test (Server)
import std.test (Tests)
import std.uri (Uri)

type async Main {
  fn async main {
    let tests = Tests.new

    tests.test('Example test', fn (t) {
      let server = Server.new(t, fn (srv) {})

      let client = Client.new

      server.prepare_client(client)

      let uri = Uri.parse('http://example.com').or_panic
      let resp = client.get(uri).send.or_panic
      let body = ByteArray.new
      let _ = resp.body.read_all(body).or_panic

      t.equal(resp.status, Status.ok)
      t.equal(body.to_string, 'hello')
    })

    tests.run
  }
}
```

This test produces the following output:

```
F

Failures:

1. Test: Example test
   Line: test_http.inko:23

     expected: 200
          got: 404

2. Test: Example test
   Line: test_http.inko:24

     expected: "hello"
          got: "No mock is defined for this request"

3. Test: Example test
   Line: test_http.inko:11

     expected: a mock matching this request
          got: GET /
               host: 0.0.0.0:34813
               user-agent: inko/0.18.1 (https://inko-lang.org)

Finished running 1 tests in 2 milliseconds, 3 failures, seed: 3246212169450956797
```

For more information on how to define mock expectations, refer to the
documentation of the [](std.net.http.test.Mock) type and its various methods.

## More information

For more information, refer to the documentation of the following:

- [](std.net.http): contains various HTTP building blocks, such as the
  [](std.net.http.Header) type
- [](std.net.http.cookie): handling of cookies for both clients and servers
- The various fields of the [](std.net.http.client.Client) type, used to
  configure the client such as the connection timeout
- [](std.net.multipart): parsing and generating of `multipart/form-data` streams
  (used by [](method://std.net.http.client.Request.multipart_form))
