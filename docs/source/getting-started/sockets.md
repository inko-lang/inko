---
{
  "title": "Hello, sockets!"
}
---

In the previous tutorial we looked at reading from and writing to files. In this
tutorial we'll instead look at reading from and writing to network sockets.

To start, change `hello.inko` from the [](hello-world) tutorial to the
following:

```inko
import std.net.ip (IpAddress)
import std.net.socket (UdpSocket)
import std.stdio (Stdout)

type async Main {
  fn async main {
    let stdout = Stdout.new
    let server = UdpSocket.new(IpAddress.v4(0, 0, 0, 0), port: 0).get
    let client = UdpSocket.new(IpAddress.v4(0, 0, 0, 0), port: 0).get
    let addr = server.local_address.get

    client.connect(addr.ip, addr.port).get
    client.write('Hello, world!').get

    let bytes = ByteArray.new

    server.read(into: bytes, size: 32).get
    stdout.write(bytes).get
  }
}
```

Then build an run it:

```bash
inko build
./build/debug/hello
```

The output will be the following:

```
Hello, world!
```

## Explanation

Compared to previous tutorials there's quite a bit going on here, so let's take
a look at what this code does.

First, we import two types not seen before: `IpAddress` and `UdpSocket`. The
first is used to represent IPv4 and IPv6 addresses, the second is used for UDP
sockets. We use UDP sockets in this example as it keeps things as simple as
possible.

Our sockets are created as follows:

```inko
let server = UdpSocket.new(IpAddress.v4(0, 0, 0, 0), port: 0).get
let client = UdpSocket.new(IpAddress.v4(0, 0, 0, 0), port: 0).get
```

What happens here is that we create two sockets that bind themselves to IP
address 0.0.0.0, using port 0. Using port 0 results in the operating system
assigning the socket a random unused port number. This way we don't need to
worry about using a port that's already in use.

Next, we encounter the following:

```inko
let addr = server.local_address.get

client.connect(addr.ip, addr.port).get
client.write('Hello, world!').get
```

Here we get the address of the server we need to connect the client to, which we
do using `connect()`. We then write the string "Hello, world!" to the client,
sending it to the server.

We then read the data back from the server:

```inko
let bytes = ByteArray.new

server.read(into: bytes, size: 32).get
stdout.write(bytes).get
```

When using sockets you shouldn't use `read_all` as we did in the files tutorial,
because `read_all` won't return until the socket is disconnected.

Just as in the files tutorial, we use `get` to handle errors for the sake of
brevity.

## Using TCP sockets

Let's change the program to use TCP sockets instead. We'll start by changing
`hello.inko` to the following:

```inko
import std.net.ip (IpAddress)
import std.net.socket (TcpServer)
import std.stdio (Stdout)

type async Main {
  fn async main {
    let stdout = Stdout.new
    let server = TcpServer.new(IpAddress.v4(0, 0, 0, 0), port: 9999).get
    let client = server.accept.get
    let bytes = ByteArray.new

    client.read(into: bytes, size: 32).get
    stdout.write(bytes).get
  }
}
```

This time we're using a fixed port number (9999) as that makes this particular
example a little easier.

Next, we'll create another file called `src/client.inko` with the following
contents:

```inko
import std.net.ip (IpAddress)
import std.net.socket (TcpClient)

type async Main {
  fn async main {
    let client = TcpClient.new([IpAddress.v4(0, 0, 0, 0)], port: 9999).get

    client.write('Hello, world!').get
  }
}
```

Now we can build both the client and server:

```bash
inko build
```

To run the client and server, we'll need two instances of our terminal. In one
instance, run `./build/debug/hello` to start the server, and in another instance
run `./build/debug/client` to start the client.

If all went well, the server writes "Hello, world!" to the terminal, then
terminates.

What we did here is create a simple TCP server using the aptly named `TcpServer`
type, connected to address 0.0.0.0 and port 9999, then connected a client to it
using the similarly aptly named `TcpClient` type.
