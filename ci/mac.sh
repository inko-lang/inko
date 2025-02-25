#!/usr/bin/env bash

set -e

LLVM_VERSION='18'
RUST_VERSION='1.78'

echo "::group::Installing Homebrew packages"
brew install llvm@${LLVM_VERSION}
echo "::endgroup::"

echo "::group::Installing Rust"
rustup-init --quiet -y --no-modify-path --profile minimal \
    --default-toolchain $RUST_VERSION
echo "::endgroup::"

echo "::group::Updating PATH"
echo "${CARGO_HOME}/bin" >> $GITHUB_PATH
echo "::endgroup::"
