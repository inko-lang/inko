# Deploying Inko programs

Inko code is run using one of two ways: `inko run` to compile and run a file, or
`inko build` to compile a file separately so you can later run it using
`inko run`. In both cases the compilation process is the same: source code is
compiled into a bytecode file (referred to as a "bytecode image"), and Inko's VM
runs this file.

When using `inko run` with a source file, the image is compiled to memory and
run. This means you need to compile the source file from scratch every time.
When using `inko build` you can save the resulting bytecode file and run it
multiple times, without having to compile your source code.

Inko's bytecode images are self-contained and don't depend on your source code
once compiled. This means that for deployments it's best/easiest to compile your
source code ahead of time, then deploy the image. You do of course still need to
install the VM in the environment you're deploying to. If your code uses Inko's
FFI to interact with C code, you'll also need to make sure the libraries used
are available in the deployment environment.

In short: during development you'll typically use `inko run` to compile and run
your code right away, but for deployments you should compile your code
separately using the `inko build` command.
