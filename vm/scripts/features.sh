#!/usr/bin/env bash

function features() {
    local cargo_cmd="${CARGO_CMD:-cargo}"

    if [[ "$(${cargo_cmd} --version)" == *nightly* ]]
    then
        echo '--features nightly'
    fi
}

features
