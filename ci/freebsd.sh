#!/usr/bin/env bash

set -e

RUST_VERSION='1.78'

echo "::group::Installing Rust"
curl --proto '=https' \
    --tlsv1.2 \
    --retry 10 \
    --retry-connrefused \
    --location \
    --silent \
    --show-error \
    --fail "https://sh.rustup.rs" | \
    sh -s -- --profile minimal -y --default-toolchain "${RUST_VERSION}"
echo "::endgroup::"

export PATH="${CARGO_HOME}/bin:${PATH}"

if [ "$1" = "compiler" ]
then
    echo "::group::Run compiler tests"
    cargo test
    echo "::endgroup::"
else
    echo "::group::Run stdlib tests"
    cd std
    cargo run -- test
    echo "::endgroup::"
fi
