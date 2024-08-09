#!/usr/bin/env bash

set -e

NEW_VERSION="${1}"

if [[ "${NEW_VERSION}" = "" ]]
then
    echo 'You must specify a new version as the first argument'
    exit 1
fi

sed -i.bak 's/^version =.*# VERSION$/version = "'"${NEW_VERSION}"'" # VERSION/' Cargo.toml
rm Cargo.toml.bak

# Make sure that Cargo.lock is also updated
if ! OUTPUT="$(cargo check 2>&1)"
then
    echo "Failed to update Cargo.lock files: ${OUTPUT}"
    exit 1
fi
