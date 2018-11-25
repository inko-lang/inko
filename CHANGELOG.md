# Changelog

## 0.3.0 - November 25, 2018

* 1411241: Use Rust's Debug to format Float
* 7ac65aa: Added Array.contains?
* 6d1d704: Removed left-over traces of the Compatible marker
* 7dc4d1c: Added tests for std::hash_map
* b750f17: Fixed error generation when parsing brackets
* d95a357: Remove uninitialised params from return types
* 8cd6b79: Handle remapping of parameters for optional types
* 739a274: Added tests for std::fs::path::Path
* 93bbd5e: Fix race condition in ObjectPointer::status()
* a8df710: Fix tests for Object::mark_for_forward()
* 21b0ab0: Use seconds instead of milliseconds for timeouts
* e2b5bff: Move interpreter code out of the main loop
* 2f57fa4: Fixed passing of excessive arguments
* 26f535f: Add a Foreign Function Interface for C code
* 9e33e40: Reformat some VM code using rustfmt
* 850824b: Improve support for pinning processes
* f39ce80: Rework handling of prototypes of built-in types
* c1c8f5d: The Platform instruction now returns a string
* 77fbe37: Move std::vm.panic to std::process
* 96c362b: Reduce moving of threads in std::stdio
* 1a6e4b4: Simplify setting the default thread counts
* 3d15c20: Supported nested calls to process.blocking
* b79e9e8: Expose thread pinning to the runtime
* faf02ea: Pinning of processes to threads
* 6d37d40: Fixed Clippy warning for Histogram::new
* 86e740f: Add tests for Histogram::reset()
* 8be5d5b: Use Chunk instead of Vec in Histogram
* 8f7c240: Implement Drop for Chunk
* 0e0b775: Use an f32 for heap growth factors
* 3e1c4c0: Use a u8 for configuring the number of threads
* bc674fa: Fixed formatting of Clippy attributes
* c5519cb: Run Clippy on Rust stable
* 709d928: Add support for custom sha256sum programs
* 72e16d4: Basic support for late binding of Self
* b0bc5e4: Fix process status in ProcessSuspendCurrent
* c983568: Fixed arity of implements_trait?
* bac0fe8: Use u16 for ExecutionContext.return_register
* 64e8a9c: Refactor std::range so it compiles again
* b1cb8a3: Support for type checking nested required traits
* ca7a359: Manually define Nil.new

## 0.2.5 - September 11, 2018

* 38379fc: Fixed type checking of unknown_message
* b29fdc0: Added std::test::assert.true and false
* ad529e5: Add base implementation of implements_trait?
* d7df0cc: Use parentheses for process.receive_if example
* ab8d7b3: Use trailing commas for arguments and literals

## 0.2.4 - September 08, 2018

### Compiler

* f827c13: Parse trailing blocks as arguments
* 3f038c6: Fix sending to types that implement UnknownMessage
* 9c4be37: Fix parsing of arguments without parenthesis
* 703ff73: Add support for deferred execution of blocks
* 6fffc53: Fix setting the receiver type of Send nodes
* 15713ea: Fix parsing backslashes in strings
* 5e6920e: Add support for registering panic handlers
* 79103c8: Explicitly bind receivers to blocks and bindings
* 69a7592: Added std::env for managing environment data

### Runtime

* 40cde2a: Remove throw requirement from Close.close
* 97ec45b: Use trailing blocks for unit tests
* 703ff73: Add support for deferred execution of blocks
* 8c06796: Return Array!(Path) in std::dir.list
* 0bbe33f: Added std::test::assert.no_panic
* 5f8d558: Rework std::test to use panics instead of throws
* 5d6d783: Add file separator constant to std::fs::path
* 9b61ffa: Added std::os.windows? to check Windows usage
* 2627f18: Return/take Path in std::env in more places
* 8fcd93e: Add support for testing panics
* 5e6920e: Add support for registering panic handlers
* 79103c8: Explicitly bind receivers to blocks and bindings
* 69a7592: Added std::env for managing environment data

### Virtual machine

* d9b6f07: Remove source level document of instructions
* 6990ca9: Remove nightly workaround for VecDeque::append()
* e2ea21d: Fixed file open modes in the VM
* 04bf29d: Update Clippy settings for latest nightly
* 703ff73: Add support for deferred execution of blocks
* db00ebb: Fix Clippy offences
* 4119eab: Remove the "locals" queue from process mailboxes
* 5a502f2: Reduce memory necessary to create processes
* bb14527: Use linked lists for process mailboxes
* 9b0105e: Fix using integers when checking prototype chains
* 5e6920e: Add support for registering panic handlers
* 79103c8: Explicitly bind receivers to blocks and bindings
* 69a7592: Added std::env for managing environment data
* 57cc1f4: Work around nightly failures for the time being
* bf55645: Added --features option to IVM
* 843aa10: Add prefetching support for stable Rust
* dd5e959: make Mailbox::mailbox_pointers unsafe
* 1caa5d0: Clean up unsafe references in the interpreter loop

### Other

* 0a517f7: Emit warnings as errors in Clippy
* 843aa10: Add prefetching support for stable Rust

## 0.2.3 - August 19, 2018

### Compiler

No changes.

### Runtime

No changes.

### Virtual machine

* 02a2cbd: Rework allocations to support parallel moving

### Other

* e672582: Moved tagging/releasing code to separate scripts
* 0888635: Use a single Make task for building a release
* 5504bb7: Use VERSION not version in "make versions"
* 019b186: Added Make task for updating version files
* 9542354: Added tooling for managing a changelog

## 0.2.2 - August 13, 2018

### Compiler

No changes.

### Runtime

No changes.

### Virtual machine

No changes.

### Other

* 11ea041: Fixed error generation in the root Makefile

## 0.2.1 - August 12, 2018

### Compiler

* fb587d8: Bump version to 0.2.1
* be2eca2: Corrected the compiler version
* 7e462a2: Expose module names and paths to the runtime
* 64afd81: Always set "self" in a lambda

### Runtime

* 6c88fa0: Added tests for std::fs
* 7e462a2: Expose module names and paths to the runtime
* 19252b8: Move debugging code from std::vm to std::debug
* 2f61662: Rename Format.format to format_for_inspect
* 6ac3b65: Fixed incorrect process.status comment

### Virtual machine

* c761817: Reformat VM code using rustfmt
* 42d8aa6: Handle integer divisions by zero explicitly
* fab3805: Fixed two Clippy warnings in interpreter

### Other

* 107dbc5: Handle Make errors in realpath.sh
* 52bd9a0: Use make -C instead of cd'ing into a directory
* 4106928: Enable rustfmt in CI
* c396f15: Run Clippy in CI
