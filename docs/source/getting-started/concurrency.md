# Concurrency

Inko allows you to perform work concurrently by using "lightweight processes".
Lightweight processes are isolated tasks, scheduled by the virtual machine.
Processes can never read each other's memory, instead they communicate by
sending messages. These messages can be any object, and they are deep copied
when sent.

Processes using isolated memory means never having to worry about data races,
nor do you need to use mutexes and similar structures. If an operation needs to
be synchronised, you can do so by sending a message to a process.

Inko uses preemptive multitasking for processes. This means that each process
runs for a certain period of time on an OS thread, after which it's suspended
and another process is allowed to run. This repeats itself until the program
finishes. Because of the use of preemptive multitasking, a single process is
unable to indefinitely block an OS thread from performing any other work.

## Sending messages

To get started with processes, you must first import the `std::process` module:

```inko
import std::process
```

This module provides a variety of methods we can use, but let's start simple:
we'll start a process, then send it a message. The started process in turn will
receive a message, then stop. First, let's start the new process:

```inko
import std::process

let proc = process.spawn {

}
```

By sending the message `spawn` to the `process` module we can start a new
process. The argument we provide is a lambda that will be executed in the newly
started process. The return value is a `Process` object, which we can later use
to send messages to the process.

Now let's change our code so that our process waits for a message to arrive:

```inko
import std::process

let proc = process.spawn {
  process.receive
}
```

Here we use `process.receive` to wait for a new message. Once received, we just
discard it.

When a process tries to receive a message, one of two things can happen:

1. If there is no message, the process will suspend itself until a message
   arrives.
1. If there is a message, the message is returned.

We haven't sent a message yet to our process, so it will suspend itself and wait
for us to send one. Let's send it a message:

```inko
import std::process

let proc = process.spawn {
  process.receive
}

proc.send('ping')
```

Here `send('ping')` sends a message to the process stored in the local variable
`proc`.

## Copying messages

When a message is sent, it's _deep copied_. This means that the sender and
receiver will both use a different copy of the data sent. Certain types are
optimised to remove the need for copying. For example, objects of type `Integer`
are not heap allocated, removing the need for copying. `String` instances use
reference counting internally, making it cheap to send a `String` from one
process to another.

When a process sends a message to itself, the message is not copied.

Despite these optimisations, it's best to avoid sending large objects to
different processes. Instead, we recommend that a single process owns the data
and sends out some kind of reference (e.g. an ID of sorts).

Having said that, copying a message is typically cheaper than using a lock of
sorts to allow concurrent access to shared memory. Furthermore, Inko tries hard
to reuse memory as best as it can. As a result, the overhead of copying
typically won't be something you should worry about.

## Waiting for a response

Our program doesn't do a whole lot: we start a process, send it a message, then
stop. Let's change our program so that the started process sends a response
back, and our main process waits for it to be received:

```inko
import std::process::(self, Process)

let child = process.spawn {
  let reply_to = process.receive as Process

  reply_to.send('pong')
}

child.send(process.current)

process.receive
```

Here we start a new process, which will then wait until it receives a process
object. Once received, it sends the `"pong"` message to it.

This is quite a bit of a jump from the previous example, so let's discuss it
step by step. We start our process as usual, which then runs the following:

```inko
import std::process::(self, Process)
```

This imports the `std::process` module and makes it available as `process`,
while also importing the `Process` constant from the same module and exposing it
as `Process`. Next, we start our process:

```inko
let child = process.spawn {
  # ...
}
```

This starts a new process, and stores the process object in the `child` local
variable. The first line this new process runs is the following:

```inko
let reply_to = process.receive as Process
```

This line of code does two things:

1. We wait for a message to arrive.
2. We inform the compiler that our message is of type `Process`.

Step one is nothing new, but step two needs some explaining. The return type of
`process.receive` is `Any`, and we can't do much with that type on its own. To
deal with this, we cast it to the type we want (`Process`).

Next, we have the following:

```inko
reply_to.send('pong')
```

Here we send a message to the process stored in `reply_to`, which in our example
is also the process that started the child process.

Next, we have the following two lines:

```inko
child.send(process.current)

process.receive
```

Here we send the child process the process object of the current process, then
wait for the child process to reply.

## Timeouts

Sometimes we want to only wait for a certain period of time when receiving a
message. We can do so by using `process.receive_timeout`:

```inko
import std::process

try! process.receive_timeout(1)
```

When running this, our program will wait one second for a message to arrive. If
no message is received in time, an error is thrown.

## Blocking operations

Sometimes a process needs to perform a task that will block the OS thread it's
running on. We can use the method `process.blocking` for this:

```inko
import std::process

process.blocking {
  # blocking operation here.
}
```

When we use `process.blocking`, the current process is moved to a separate
thread pool dedicated to slow or blocking processes. This allows us to perform
our blocking operation (in the provided block), while still allowing other
processes to run without getting blocked as well.

Typically you won't have to use `process.blocking` as the various Inko APIs will
take care of this for you. For example, various file system operations use
`process.blocking` to move blocking operations to the separate thread pool.

## Process monitoring

If you have worked with Erlang or Elixir before, you may wonder if there is a
way to monitor a process. There isn't. Inko's error handling model prevents
unexpected runtime errors from occurring, removing the need for process
monitoring. Panics in turn stop the entire program by default, and are not meant
to be monitored from another Inko process, as panics are the result of software
bugs, and software bugs should not be ignored or retried.
