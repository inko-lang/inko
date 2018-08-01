#!/usr/bin/env bash

if [[ "${APPVEYOR_REPO_TAG}" == '' ]]
then
    echo 'Not pushing to a tag, would normally skip'
fi

echo "Building release for $(bash scripts/arch.sh)"

echo 'Building compiled release...'
make release-compiled
