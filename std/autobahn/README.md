# WebSocket tests using Autobahn

This directory contains a few files used for running WebSocket tests using
[Autobahn](https://github.com/crossbario/autobahn-testsuite/). These tests are
run manually as we have our own tests as part of CI, and mainly serve as
"inspiration" for our own tests.

For the server tests a few tests related to message sizes are disabled. This is
because we enforce a default message size limit smaller than several of the
Autobahn tests expect. Compression tests are disabled because WebSocket
compression isn't implemented.

To test a WebSocket server, run `bash server.sh` in this directory. The results
are stored in `/tmp/autobahn`.
