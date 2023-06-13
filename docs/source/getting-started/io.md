# Asynchronous IO

IO operations, such as writing to a file or reading from a socket, are
asynchronous in Inko. Unlike other languages, there is no
[async/await](https://en.wikipedia.org/wiki/Async/await) and no [function
colouring](http://journal.stuffwithstuff.com/2015/02/01/what-color-is-your-function/).
Inko achieves this by baking asynchronous IO into the language, much like
languages such as [Erlang](https://www.erlang.org/) and [Go](https://go.dev/).

In plain English: Inko's runtime ensures an IO operation can't prevent other
processes from running.

## Sockets

Sockets are implemented as non-blocking sockets. When performing an operation
that would block, the process is suspended. A background thread called the
"network poller" then keeps an eye on the process, rescheduling it when the
operation is ready. The network poller uses epoll on Linux, and kqueue on macOS
and the various BSDs.

By default a single network poller thread is used, but the amount is
configurable using the
[`INKO_NETPOLL_THREADS`](../../guides/scaling/#environment-variables)
environment variable.

Sockets are provided by the module `std::net::socket`. The following socket
types exist in this module:

- `Socket`: a low-level IPv4/IPv6 socket. You probably don't want to use this
  directly unless necessary.
- `UdpSocket`: a UDP IPv4/IPv6 socket.
- `TcpClient`: an IPv4/IPv6 TCP stream socket acting as a client.
- `TcpServer`: an IPv4/IPv6 TCP stream socket acting as a server.
- `UnixSocket`: a low-level Unix domain socket.
- `UnixDatagram`: a Unix datagram socket.
- `UnixClient`: a Unix stream socket acting as a client.
- `UnixServer`: a Unix stream socket acting as a server.

## Files and standard input/output

Other IO operations that don't support non-blocking operations, such as reading
from a file or writing to STDERR, use a different approach to ensure they don't
block the OS thread.

When such an operation is performed, Inko's runtime keeps track of how long the
operation is running for. If this takes too long, a backup OS thread is woken up
and takes over the work of the OS thread performing the blocking operation. When
the blocked thread wakes up again it reschedules the process, then turns itself
into a backup thread.

The amount of backup threads used is configured using the
[`INKO_BACKUP_THREADS`](../../guides/scaling/#environment-variables) environment
variable.

### Files

Types for working with files are provided in the module `std::fs::file`. The
following types are provided:

- `ReadOnlyFile`: opens a file that only allows reads
- `WriteOnlyFile`: opens a file that only allows writes
- `ReadWriteFile`: opens a file that allows both reads and writes

Instances of these types are created using the static `new` method, for example:

```inko
import std::fs::file::WriteOnlyFile

WriteOnlyFile.new('test.txt').expect('failed to open the file')
```

`WriteOnlyFile` and `ReadWriteFile` place the file cursor at the start of the
file, overwriting existing content when writing. To instead append to the end of
the file, use the `append` static method:

```inko
import std::fs::file::WriteOnlyFile

WriteOnlyFile.append('test.txt').expect('failed to open the file')
```

### Standard input/output

The module `std::stdio` provides types for working with standard input/output
streams. These types are as follows:

- `STDIN`: a type for reading from the standard input stream.
- `STDOUT`: a type for writing to the standard output stream.
- `STDERR`: a type for writing to the standard error stream.

## Other IO types

The module `std::io` provides various traits implemented by other IO types. For
example, the `Read` trait is implemented by IO types that support reads.
