#!/usr/bin/env bash

set -e

function check() {
    local compiler_version repo_version vm_version

    repo_version="$(cat VERSION)"
    compiler_version="$(grep -oP "VERSION\\s*=\\s*'(\\K[^']+)" compiler/lib/inkoc/version.rb)"
    vm_version="$(grep VERSION vm/Cargo.toml | grep -oP 'version\s*=\s*"(\K[\d\.]+)')"

    if [[ "${compiler_version}" != "${repo_version}" ]]
    then
        echo "Incorrect compiler version, expected ${repo_version} but got ${compiler_version}"
        exit 1
    fi

    if [[ "${vm_version}" != "${repo_version}" ]]
    then
        echo "Incorrect VM version, expected ${repo_version} but got ${vm_version}"
        exit 1
    fi

    echo 'OK: all versions match'
}

check
