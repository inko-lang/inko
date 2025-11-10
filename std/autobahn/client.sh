#!/usr/bin/env bash

set -e

mkdir -p /tmp/autobahn
podman run --interactive --tty --rm \
    --volume ${PWD}/config:/config:z \
    --volume /tmp/autobahn:/reports:z \
    --publish 8001:8001 \
    docker.io/crossbario/autobahn-testsuite wstest \
    -m fuzzingserver \
    -s /config/server.json
