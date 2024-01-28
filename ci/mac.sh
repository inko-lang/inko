#!/usr/bin/env bash

set -e

LLVM_VERSION='15'
RUST_VERSION='1.70'

echo "::group::Installing Homebrew packages"
brew install llvm@${LLVM_VERSION} rustup-init
echo "::endgroup::"

echo "::group::Installing Rust"
rustup-init --quiet -y --no-modify-path --profile minimal \
    --default-toolchain $RUST_VERSION
echo "::endgroup::"

echo "::group::Updating PATH"
echo "$(brew --prefix llvm@${LLVM_VERSION})/bin" >> $GITHUB_PATH
echo "${CARGO_HOME}/bin" >> $GITHUB_PATH
echo "::endgroup::"
