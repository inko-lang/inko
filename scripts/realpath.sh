#!/usr/bin/env bash

function real-path() {
    if [[ -x "$(command -v realpath)" ]]
    then
        realpath -m "${1}"
    elif [[ -x "$(command -v grealpath)" ]]
    then
        grealpath -m "${1}"
    else
        echo 'Could not find (g)realpath, make sure (GNU) coreutils is installed'
        exit 1
    fi
}

real-path "$@"
