#!/usr/bin/env bash

set -e

if [ "${1}" = '' ]
then
    echo 'A Rust version is required'
    exit 1
fi

echo "::group::Installing Rust ${1}"

export PATH="${CARGO_HOME}/bin:${PATH}"
echo "${CARGO_HOME}/bin" >> $GITHUB_PATH

if ! command -v rustup &>/dev/null
then
    curl --proto '=https' \
    --tlsv1.2 \
    --retry 10 \
    --retry-connrefused \
    --location \
    --silent \
    --show-error \
    --fail "https://sh.rustup.rs" | \
    sh -s -- --profile minimal -y --default-toolchain none
fi

rustup toolchain install "${1}" --profile minimal --no-self-update
rustup default "${1}"

echo "::endgroup::"
