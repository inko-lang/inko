#!/usr/bin/env bash

set -e

inko build
./build/debug/server &
server_pid=$!

mkdir -p /tmp/autobahn
podman run --interactive --tty --rm \
    --volume ${PWD}/config:/config:z \
    --volume /tmp/autobahn:/reports:z \
    --publish 8000:8000 \
    docker.io/crossbario/autobahn-testsuite wstest \
    -m fuzzingclient \
    -s /config/client.json

kill "${server_pid}"
wait "${server_pid}"
