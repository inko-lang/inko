#!/usr/bin/env bash

set -e

function print-arch {
    local os
    local arch
    local env

    os="$(rustc --print cfg | grep -oP 'target_os="\K(\w+)(?=")')"
    arch="$(rustc --print cfg | grep -oP 'target_arch="\K(\w+)(?=")')"
    env="$(rustc --print cfg | grep -oP 'target_env="\K(\w+)(?=")')"

    echo "${arch}-${os}-${env}"
}

print-arch
