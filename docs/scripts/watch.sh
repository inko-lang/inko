#!/usr/bin/env bash

function build {
    inko build --release && ./build/release/main
}

build
python -m http.server -d public &
python_pid=$!

trap 'kill ${python_pid}; exit' INT

while inotifywait --recursive \
    --event modify \
    --event create \
    --event delete \
    --event move \
    -qq \
    --exclude '^\.\/(build|public)' \
    .
do
    build
done

wait "${python_pid}"
