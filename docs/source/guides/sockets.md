# Sockets

Sockets are used to communicate with other programs over the internet. Inko
provides the following modules for using sockets:

| Module             | Provides
|:-------------------|:--------------------------------------------------------
| `std::net::socket` | TCP and UDP sockets
| `std::net::unix`   | UNIX sockets
| `std::net::ip`     | Types for IPv4 and IPv6 addresses

The modules `std::net::socket` and `std::net::unix` share a similar layout: they
both offer a low-level `Socket` type, and various high-level types such as
`UdpSocket` and `UnixDatagram`. These high-level types allow you to get the
low-level `Socket` type they wrap, which can be used for setting options such as
the TCP keep-alive time.

## TCP clients and servers

We'll start with a simple example: a TCP server that accepts incoming
connections and writes a response, and a TCP client that connects to this server
and sends a message. We will start by importing the necessary types:

```inko
import std::net::socket::(TcpListener, TcpStream)
```

The `TcpListener` type will be used as our TCP server. The `TcpStream` is our
TCP client, connecting to the `TcpListener`. We do so as follows:

```inko
import std::net::socket::(TcpListener, TcpStream)

let listener = try! TcpListener.new(ip: '127.0.0.1', port: 40_000)
let stream = try! TcpStream.new(ip: '127.0.0.1', port: 40_000)
```

Here we create a `TcpListener` listening on address `127.0.0.1`, port `40 000`.
The stream connects to the same address. We're using `try!` here so that any
errors will result in a panic, stopping the program.

With the listener and stream in place, let's write some data to the stream:

```inko
import std::net::socket::(TcpListener, TcpStream)

let listener = try! TcpListener.new(ip: '127.0.0.1', port: 40_000)
let stream = try! TcpStream.new(ip: '127.0.0.1', port: 40_000)

try! stream.write_string('ping')
```

Here we write the string `'ping'` to the stream, using `try!` to panic if an
error were to occur.

To accept a new connection, send `accept` to a `TcpListener`:

```inko
import std::net::socket::(TcpListener, TcpStream)

let listener = try! TcpListener.new(ip: '127.0.0.1', port: 40_000)
let stream = try! TcpStream.new(ip: '127.0.0.1', port: 40_000)

try! stream.write_string('ping')

let connection = try! listener.accept
```

The method `TcpListener.accept` returns a `TcpStream` that can be read from and
written to. With the connection in place, we can read the message sent earlier:

```inko
import std::net::socket::(TcpListener, TcpStream)
import std::stdio::stdout

let listener = try! TcpListener.new(ip: '127.0.0.1', port: 40_000)
let stream = try! TcpStream.new(ip: '127.0.0.1', port: 40_000)

try! stream.write_string('ping')

let connection = try! listener.accept
let message = try! connection.read_string(4)

stdout.print(message)
```

Here we use `TcpListener.read_string` to read the message into a `String`. We
could also use `TcpListener.read_bytes` if we wanted to read the data into an
existing `ByteArray`.

Running the code we have written so far will result in "ping" being written to
STDOUT.

## Unix socket clients and servers

Unix domain sockets are provided by the module `std::net::unix` and provide an
interface similar as `std::net::socket`. The TCP example shown above would look
as follows when using Unix domain sockets:

```inko
import std::net::unix::(UnixListener, UnixStream)
import std::stdio::stdout

let listener = try! UnixListener.new('/tmp/test.sock')
let stream = try! UnixStream.new('/tmp/test.sock')

try! stream.write_string('ping')

let connection = try! listener.accept
let message = try! connection.read_string(4)

stdout.print(message)
```

Keep in mind that closing a `UnixListener` does not automatically remove the
socket file, so you have to do so manually if you want to run the above code
more than once.

## Handling blocking operations

The socket APIs provided by Inko are built on top of non-blocking sockets, but
without the need for using callbacks or promises. This allows you to write code
in a linear and easy to understand way, without sacrificing performance.

This means you don't have to (and should not) use `std::process.blocking` when
using the socket APIs provided by Inko.

## Parsing IP addresses

The module `std::net::ip` is used to generate and parse IPv4 and IPv6 addresses.
For example, we can parse an IP address as follows:

```inko
import std::net::ip

let address = try! ip.parse('1.2.3.4')
```

This would produce an instance of the `Ipv4Address` and store it in the
`address` local variable. You can also convert a `String` to an IP address by
importing `std::net::ip` and sending `to_ip_address` to a `String`:

```inko
import std::net::ip

let address = try! '1.2.3.4'.to_ip_address
```

You can also create IPv4 and IPv6 addresses yourself:

```inko
import std::net::ip::(Ipv4Address, Ipv6Address)

# For the IPv4 address '127.0.0.1':
Ipv4Address.new(127, 0, 0, 1)

# For the IPv6 address '::1':
Ipv6Address.new(0, 0, 0, 0, 0, 0, 0, 1)
```

Both these types implement the `IpAddress` trait. These types can also be
converted back to a `String` by sending `to_string` to them:

```inko
import std::net::ip::(Ipv4Address, Ipv6Address)

Ipv4Address.new(127, 0, 0, 1).to_string           # => '127.0.0.1'
Ipv6Address.new(0, 0, 0, 0, 0, 0, 0, 1).to_string # => '::1'
```

## Sending sockets across processes

Sockets can be sent from one process to another. This allows you to write code
that accepts incoming connections in one process, then sends those sockets to a
separate processes. This allows us to write a simple HTTP server that uses
separate processes for accepting requests and writing a response:

```inko
import std::net::socket::(TcpListener, TcpStream)
import std::process
import std::string_buffer::StringBuffer

let listener = try! TcpListener.new(ip: '127.0.0.1', port: 8080)

{
  let client = try! listener.accept
  let proc = process.spawn {
    let client = process.receive as TcpStream
    let reply = 'Hello, HTTP!'
    let output = StringBuffer.new(
      "HTTP/1.1 200 OK\r\n",
      "Content-Type: text/plain\r\n",
      'Content-Length: ',
      reply.bytesize.to_string,
      "\r\n",
      "Connection: close\r\n",
      "\r\n",
      reply
    )

    try! client.write_string(output.to_string)
    try! client.shutdown

    # While the socket will be closed when it is garbage collected, this may
    # take a little while, so we close it right away.
    client.close
  }

  proc.send(client)

  # Since the socket is copied, we need to close it here so we don't run out of
  # file descriptors.
  client.close
}.loop
```

You can then send requests to it using curl like so:

```bash
curl http://127.0.0.1:8080
```

We can also send the `TcpListener` to different processes, allowing different
processes to accept incoming connections in parallel:

```inko
import std::net::socket::(TcpListener, TcpStream)
import std::process
import std::string_buffer::StringBuffer

let listener = try! TcpListener.new(ip: '127.0.0.1', port: 8080)
let mut to_start = 4

{ to_start.positive? }.while_true {
  let proc = process.spawn {
    let listener = process.receive as TcpListener
    let reply = 'Hello, HTTP!'

    {
      let client = try! listener.accept
      let output = StringBuffer.new(
        "HTTP/1.1 200 OK\r\n",
        "Content-Type: text/plain\r\n",
        'Content-Length: ',
        reply.bytesize.to_string,
        "\r\n",
        "Connection: close\r\n",
        "\r\n",
        reply
      )

      try! client.write_string(output.to_string)
      try! client.shutdown

      client.close
    }.loop
  }

  proc.send(listener)

  to_start -= 1
}

# This prevents the program from terminating right away, instead requiring the
# user to manually terminate it.
process.receive
```
