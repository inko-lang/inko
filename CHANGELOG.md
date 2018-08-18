# Changelog

## 0.2.3 - August 19, 2018

### Compiler

No changes.

### Runtime

No changes.

### Virtual machine

No changes.

### Other

* e672582: Moved tagging/releasing code to separate scripts
* 0888635: Use a single Make task for building a release

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
