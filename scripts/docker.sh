#!/usr/bin/env bash

set -e

tag="$1"

if [ "$tag" = '' ]
then
    echo 'You must specify the tag name as the first argument.'
    exit 1
fi

version="${tag/v/}"

echo "$DOCKER_HUB_TOKEN" | \
    docker login --password-stdin --username "$DOCKER_HUB_USER" \
    docker.io

docker build -t "inkolang/inko:$version" -f Dockerfile .
docker push "inkolang/inko:$version"
docker logout docker.io
