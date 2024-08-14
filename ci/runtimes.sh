#!/usr/bin/env bash

set -e

RUST_VERSION='1.78'
DIR="tmp/runtimes"

function rustup_lib {
    local home
    local toolchain
    home="$(rustup show home)"
    toolchain="$(rustup show active-toolchain | awk '{print $1}')"

    echo "${home}/toolchains/${toolchain}/lib/rustlib/${1}/lib/"
}

function build {
    local rust_target
    local inko_target
    local out
    local target_dir
    rust_target="${1}"
    inko_target="${2}"
    out="${DIR}/${inko_target}.tar.gz"
    target_dir="${DIR}/${inko_target}"

    if [[ -f "${out}" ]]
    then
        return 0
    fi

    rustup target add "${rust_target}"
    cargo build -p rt --release --target="${rust_target}"
    mkdir -p "${target_dir}"
    cp "target/${rust_target}/release/libinko.a" "${target_dir}"

    if [[ "${rust_target}" == *-musl ]]
    then
        cp "$(rustup_lib "${rust_target}")/self-contained/libunwind.a" \
            "${target_dir}"
    fi

    tar --directory "${DIR}" --create --gzip --file "${out}" "${inko_target}"
    rm -rf "${target_dir}"
}

function install_rust {
    curl --proto '=https' \
        --tlsv1.2 \
        --retry 10 \
        --retry-connrefused \
        --location \
        --silent \
        --show-error \
        --fail "https://sh.rustup.rs" | \
        sh -s -- --profile minimal -y --default-toolchain "${RUST_VERSION}"

    export PATH="${CARGO_HOME}/bin:${PATH}"
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
    "amd64-mac")
        build "x86_64-apple-darwin" "amd64-mac-native"
    ;;
    "arm64-mac")
        build "aarch64-apple-darwin" "arm64-mac-native"
    ;;
    "amd64-freebsd")
        install_rust
        build "x86_64-unknown-freebsd" "amd64-freebsd-native"
    ;;
    *)
        echo "the architecture '$1' is invalid"
        exit 1
    ;;
esac
