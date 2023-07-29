#!/usr/bin/env bash

set -e

echo "::group::Installing LLVM"

if [ "${1}" = "ubuntu" ]
then
    # libclang-common is needed because of
    # https://gitlab.com/taricorp/llvm-sys.rs/-/issues/16.
    sudo apt-get update
    sudo apt-get install --yes llvm-15 llvm-15-dev libstdc++-11-dev \
        libclang-common-15-dev zlib1g-dev
elif [ "${1}" = "mac" ]
then
    brew install llvm@15
    echo "$(brew --prefix llvm@15)/bin" >> $GITHUB_PATH
else
    echo 'An OS to install LLVM for must be specified'
    exit 1
fi

echo "::endgroup::"
