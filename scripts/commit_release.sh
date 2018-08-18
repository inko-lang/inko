#!/usr/bin/env bash

set -e

if [[ "${VERSION}" == '' ]]
then
    echo 'You must specify a version in the VERSION environment variable.'
    exit 1
fi

git commit VERSION \
    compiler/lib/inkoc/version.rb vm/Cargo.toml vm/Cargo.lock CHANGELOG.md \
    -m "Release v${VERSION}"

git push origin "$(git rev-parse --abbrev-ref HEAD)"
