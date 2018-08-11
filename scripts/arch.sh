#!/usr/bin/env bash

set -e

function rustc_cfg() {
    rustc --print cfg | grep "$1" | cut -d '=' -f 2 | cut -d '"' -f 2
}

function print-arch {
    local os
    local arch
    local env

    os="$(rustc_cfg target_os)"
    arch="$(rustc_cfg target_arch)"
    env="$(rustc_cfg target_env)"

    echo "${arch}-${os}-${env}"
}

print-arch
