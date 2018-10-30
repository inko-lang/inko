#!/usr/bin/env bash

set -e

cd "$APPVEYOR_BUILD_FOLDER" || exit 1

echo 'Running compiler tests...'
make -C compiler test

echo 'Running VM tests...'
make -C vm test

echo 'Building the VM...'
make -C vm release

# Run the runtime tests
ruby -I ./compiler/lib ./compiler/bin/inko-test -d runtime \
    --vm vm/target/release/ivm
