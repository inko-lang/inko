#!/usr/bin/env bash

set -e

INKO_VERSION="$(cargo pkgid -p inko | cut -d\# -f2 | cut -d: -f2)"
INKO_TRIPLE="${1}"
RUST_TRIPLE="${2}"
INKO_RUNTIMES="${HOME}/.local/share/inko/runtimes/${INKO_VERSION}/${INKO_TRIPLE}"

function rustup_lib {
    local home
    local toolchain
    home="$(rustup show home)"
    toolchain="$(rustup show active-toolchain | awk '{print $1}')"

    echo "${home}/toolchains/${toolchain}/lib/rustlib/${1}/lib/"
}

# As the host is a GNU host, a bit of extra work is needed to get
# cross-compilation to musl to work.
if [[ "${INKO_TRIPLE}" == *-musl ]]
then
    cargo build -p rt --target="${RUST_TRIPLE}"
    mkdir -p "${INKO_RUNTIMES}"
    cp "./target/${RUST_TRIPLE}/debug/libinko.a" "${INKO_RUNTIMES}/libinko.a"
    cp "$(rustup_lib "${RUST_TRIPLE}")/self-contained/libunwind.a" \
        "${INKO_RUNTIMES}/libunwind.a"

    INKO_RT="${PWD}/target/${RUST_TRIPLE}/debug" cargo build -p inko
else
    cargo build
fi
