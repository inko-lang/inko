#!/usr/bin/env bash

set -e

if [ "$RUNNER_OS" = "Linux" ]
then
    # libclang-common is needed because of
    # https://gitlab.com/taricorp/llvm-sys.rs/-/issues/16.
    sudo apt-get update
    sudo apt-get install --yes llvm-15 llvm-15-dev libstdc++-11-dev \
        libclang-common-15-dev zlib1g-dev
elif [ "$RUNNER_OS" = "macOS" ]
then
    brew install llvm@15
    echo "$(brew --prefix llvm@15)/bin" >> $GITHUB_PATH
else
    echo 'RUNNER_OS must be set to a supported value'
    exit 1
fi
