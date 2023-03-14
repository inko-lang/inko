#!/bin/sh

set -e

image_name="$1"

if [ "$image_name" = '' ]
then
    echo 'You must specify the image name as the first argument.'
    exit 1
fi

docker build -t "${image_name}" -f Dockerfile .

docker login \
    --username gitlab-ci-token \
    --password "${CI_JOB_TOKEN}" \
    "$CI_REGISTRY"

docker push "${image_name}"
docker logout "$CI_REGISTRY"
