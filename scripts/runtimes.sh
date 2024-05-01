#!/usr/bin/env bash

set -e

# We use a nightly version as close as possible to the minimum Rust version that
# we require. This version is obtained by going through the releases at
# https://releases.rs/ and picking a nightly close to the branch date of the
# desired release.
#
# This is needed so we can use the unwinding crate while still having a somewhat
# stable version of Rust, instead of relying on the latest nightly version that
# may randomly change.

# The Inko version we're building the runtimes for.
VERSION="${1}"

# The directory to place the runtimes in.
DIR="tmp/runtimes/${VERSION}"

# The Cloudflare bucket to store the runtime files in.
BUCKET="inko-releases"

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

if [[ "${VERSION}" = "" ]]
then
    echo 'You must specify the Inko version to build the runtimes for'
    exit 1
fi

mkdir -p "${DIR}"

# FreeBSD
build "x86_64-unknown-freebsd" "amd64-freebsd-native"

# Linux
build "x86_64-unknown-linux-gnu" "amd64-linux-gnu"
build "x86_64-unknown-linux-musl" "amd64-linux-musl"
build "aarch64-unknown-linux-gnu" "arm64-linux-gnu"
build "aarch64-unknown-linux-musl" "arm64-linux-musl"

# macOS
build "x86_64-apple-darwin" "amd64-mac-native"
build "aarch64-apple-darwin" "arm64-mac-native"

# Upload the results to the bucket.
rclone sync --config rclone.conf --checksum --verbose \
    "${DIR}" "production:${BUCKET}/runtimes/${VERSION}"
