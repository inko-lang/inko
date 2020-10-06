#!/usr/bin/env bash

set -e

function check() {
    local compiler_version cli_version vm_version

    cli_version="$(cargo pkgid -p inko | cut -d# -f2 | cut -d: -f2)"
    compiler_version="$(grep -oP "VERSION\\s*=\\s*'(\\K[^']+)" compiler/lib/inkoc/version.rb)"
    vm_version="$(cargo pkgid -p libinko | cut -d# -f2 | cut -d: -f2)"

    if [[ "${compiler_version}" != "${cli_version}" ]]
    then
        echo "Incorrect compiler version, expected ${cli_version} but got ${compiler_version}"
        exit 1
    fi

    if [[ "${vm_version}" != "${cli_version}" ]]
    then
        echo "Incorrect VM version, expected ${cli_version} but got ${vm_version}"
        exit 1
    fi

    echo 'OK: all versions match'
}

check
