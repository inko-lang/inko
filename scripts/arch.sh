#!/usr/bin/env bash

set -e

if [[ $(rustc --print cfg | grep 'target_env="musl"') == *"musl"* ]]
then
    echo "$(uname -m)-musl"
else
    uname -m
fi
