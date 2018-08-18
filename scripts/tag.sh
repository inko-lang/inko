#!/usr/bin/env bash

set -e

if [[ "${VERSION}" == '' ]]
then
    echo 'You must specify a version in the VERSION environment variable.'
    exit 1
fi

git tag -a -m "Release v${VERSION}" "v${VERSION}"
git push origin "v${VERSION}"
