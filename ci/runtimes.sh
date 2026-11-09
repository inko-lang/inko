#!/usr/bin/env bash

set -e

DIR="tmp/runtimes"

function build {
    local rust_target
    local inko_target
    local out
    local target_dir
    rust_target="${1}"
    inko_target="${2}"
    out="${DIR}/${inko_target}.tar.gz"
    target_dir="${DIR}/${inko_target}"

    cargo build -p rt --release
    mkdir -p "${target_dir}"
    cp "target/${rust_target}/release/libinko.a" "${target_dir}"
    tar --directory "${DIR}" --create --gzip --file "${out}" "${inko_target}"
    rm -rf "${target_dir}"
}

mkdir -p "${DIR}"

case "$1" in
    "amd64-linux")
        build "x86_64-unknown-linux-gnu" "amd64-linux-gnu"
        build "x86_64-unknown-linux-musl" "amd64-linux-musl"
    ;;
    "arm64-linux")
        build "aarch64-unknown-linux-gnu" "arm64-linux-gnu"
        build "aarch64-unknown-linux-musl" "arm64-linux-musl"
    ;;
    "arm64-mac")
        build "aarch64-apple-darwin" "arm64-mac-native"
    ;;
    "amd64-freebsd")
        build "x86_64-unknown-freebsd" "amd64-freebsd-native"
    ;;
    "arm64-freebsd")
        build "aarch64-unknown-freebsd" "arm64-freebsd-native"
    ;;
    *)
        echo "the architecture '$1' is invalid"
        exit 1
    ;;
esac
