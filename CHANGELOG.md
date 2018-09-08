# Changelog

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
